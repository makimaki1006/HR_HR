//! `HubSpotClient` のテスト。
//!
//! 本物の HubSpot には通信しない。127.0.0.1 のランダムポートに axum で偽 HubSpot を立て、
//! 受け取ったリクエスト (メソッド・パス・query・Authorization・本文・受信時刻) を記録する。

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::Response,
    Router,
};
use serde_json::{json, Value};

use super::{ClientOptions, HubSpotClient};
use crate::hubspot::types::{HubSpotError, RateLimitSnapshot};

const TOKEN: &str = "test-token-XYZ";

// ---------------------------------------------------------------------------
// 偽 HubSpot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    auth: Option<String>,
    body: Value,
    at: Instant,
}

struct Resp {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: String,
    delay: Duration,
}

impl Resp {
    fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.to_string(),
            delay: Duration::ZERO,
        }
    }
    fn header(mut self, k: &'static str, v: &str) -> Self {
        self.headers.push((k, v.to_string()));
        self
    }
    fn delay(mut self, d: Duration) -> Self {
        self.delay = d;
        self
    }
}

/// 呼び出し番号 (0 始まり) と受信内容から応答を決める
type Responder = Arc<dyn Fn(usize, &Recorded) -> Resp + Send + Sync>;

struct Fake {
    calls: Mutex<Vec<Recorded>>,
    responder: Responder,
}

impl Fake {
    fn calls(&self) -> Vec<Recorded> {
        self.calls.lock().unwrap().clone()
    }
    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

async fn handle(State(fake): State<Arc<Fake>>, req: Request<Body>) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 10 * 1024 * 1024).await.unwrap();
    let query = parts
        .uri
        .query()
        .map(|q| {
            reqwest::Url::parse(&format!("http://x/?{q}"))
                .unwrap()
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();
    let rec = Recorded {
        method: parts.method.to_string(),
        path: parts.uri.path().to_string(),
        query,
        auth: parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        at: Instant::now(),
    };
    let idx = {
        let mut calls = fake.calls.lock().unwrap();
        calls.push(rec.clone());
        calls.len() - 1
    };
    let resp = (fake.responder)(idx, &rec);
    if !resp.delay.is_zero() {
        tokio::time::sleep(resp.delay).await;
    }
    let mut b = Response::builder()
        .status(StatusCode::from_u16(resp.status).unwrap())
        .header("content-type", "application/json");
    for (k, v) in &resp.headers {
        b = b.header(*k, v);
    }
    b.body(Body::from(resp.body)).unwrap()
}

async fn spawn_fake(responder: Responder) -> (String, Arc<Fake>) {
    let fake = Arc::new(Fake {
        calls: Mutex::new(Vec::new()),
        responder,
    });
    let router = Router::new().fallback(handle).with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), fake)
}

fn fast_opts() -> ClientOptions {
    ClientOptions {
        timeout: Duration::from_millis(300),
        max_retries: 2,
        retry_base_delay: Duration::from_millis(20),
        search_min_interval: Duration::from_millis(60),
        rate_limited_min_wait: Duration::from_millis(20),
    }
}

fn client(base: &str, opts: ClientOptions) -> HubSpotClient {
    HubSpotClient::new(TOKEN.to_string(), base, opts).unwrap()
}

fn contact_json() -> Value {
    json!({
        "id": "101",
        "properties": {
            "firstname": "山田",
            "email": "yamada@example.com",
            "phone": null
        },
        "createdAt": "2026-01-02T03:04:05.000Z",
        "updatedAt": "2026-09-01T00:00:00.000Z",
        "archived": false
    })
}

fn always(status: u16, body: Value) -> Responder {
    Arc::new(move |_, _| Resp::json(status, body.clone()))
}

// ---------------------------------------------------------------------------
// 1. 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_object_parses_values_and_sends_bearer_and_properties() {
    let (base, fake) = spawn_fake(always(200, contact_json())).await;
    let c = client(&base, fast_opts());

    let rec = c
        .get_object("contacts", "101", &["firstname", "email", "phone"])
        .await
        .unwrap();

    assert_eq!(rec.id, "101");
    assert_eq!(
        rec.properties.get("firstname"),
        Some(&Some("山田".to_string()))
    );
    assert_eq!(
        rec.properties.get("email"),
        Some(&Some("yamada@example.com".to_string()))
    );
    assert_eq!(rec.properties.get("phone"), Some(&None));
    assert_eq!(rec.created_at.as_deref(), Some("2026-01-02T03:04:05.000Z"));
    assert_eq!(rec.updated_at.as_deref(), Some("2026-09-01T00:00:00.000Z"));
    assert!(!rec.archived);

    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
    assert_eq!(calls[0].path, "/crm/v3/objects/contacts/101");
    assert_eq!(calls[0].auth.as_deref(), Some("Bearer test-token-XYZ"));
    assert_eq!(
        calls[0].query,
        vec![(
            "properties".to_string(),
            "firstname,email,phone".to_string()
        )]
    );
}

#[tokio::test]
async fn blank_token_is_not_configured() {
    let err =
        HubSpotClient::new("  \t ".to_string(), "http://127.0.0.1:1", fast_opts()).unwrap_err();
    assert_eq!(err, HubSpotError::NotConfigured);
}

// ---------------------------------------------------------------------------
// 2. 429 → 200 / 12. 逆証明 (max_retries=0)
// ---------------------------------------------------------------------------

fn rate_limited_then_ok() -> Responder {
    Arc::new(|i, _| {
        if i == 0 {
            Resp::json(
                429,
                json!({"status": "error", "message": "echo test-token-XYZ"}),
            )
            .header("retry-after", "0")
        } else {
            Resp::json(200, contact_json())
        }
    })
}

#[tokio::test]
async fn retries_429_then_succeeds() {
    let (base, fake) = spawn_fake(rate_limited_then_ok()).await;
    let c = client(&base, fast_opts());
    let rec = c.get_object("contacts", "101", &["email"]).await.unwrap();
    assert_eq!(
        rec.properties.get("email"),
        Some(&Some("yamada@example.com".to_string()))
    );
    assert_eq!(fake.count(), 2);
}

/// 逆証明: 同じ偽サーバでも retry を 0 にすると RateLimited で終わる
/// (= 上のテストの成功は retry によるもの)
#[tokio::test]
async fn reverse_proof_no_retry_gives_rate_limited() {
    let (base, fake) = spawn_fake(rate_limited_then_ok()).await;
    let c = client(
        &base,
        ClientOptions {
            max_retries: 0,
            ..fast_opts()
        },
    );
    let err = c
        .get_object("contacts", "101", &["email"])
        .await
        .unwrap_err();
    assert_eq!(err, HubSpotError::RateLimited);
    assert_eq!(fake.count(), 1);
}

#[tokio::test]
async fn retry_after_is_honored_over_base_delay() {
    // base delay は 1ms。Retry-After: 1 (秒) を守れば 2 回目の受信は 1 秒以上後
    let responder: Responder = Arc::new(|i, _| {
        if i == 0 {
            Resp::json(429, json!({})).header("retry-after", "1")
        } else {
            Resp::json(200, contact_json())
        }
    });
    let (base, fake) = spawn_fake(responder).await;
    let c = client(
        &base,
        ClientOptions {
            retry_base_delay: Duration::from_millis(1),
            ..fast_opts()
        },
    );
    c.get_object("contacts", "101", &[]).await.unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].at.duration_since(calls[0].at) >= Duration::from_millis(1000));
    // properties 未指定なら query を付けない
    assert!(calls[0].query.is_empty());
}

/// 429 で Retry-After が無いときは base delay (1ms) ではなく rate_limited_min_wait (300ms) 待つ。
/// 5xx は base delay のまま (最低待ちは 429 だけ)。
#[tokio::test]
async fn rate_limited_without_retry_after_waits_at_least_min_wait() {
    let responder: Responder = Arc::new(|i, rec| {
        let first_status = if rec.path.ends_with("/101") { 429 } else { 503 };
        if i % 2 == 0 {
            Resp::json(first_status, json!({}))
        } else {
            Resp::json(200, contact_json())
        }
    });
    let (base, fake) = spawn_fake(responder).await;
    let c = client(
        &base,
        ClientOptions {
            retry_base_delay: Duration::from_millis(1),
            rate_limited_min_wait: Duration::from_millis(300),
            ..fast_opts()
        },
    );
    c.get_object("contacts", "101", &[]).await.unwrap();
    c.get_object("contacts", "102", &[]).await.unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 4);
    let gap_429 = calls[1].at.duration_since(calls[0].at);
    let gap_503 = calls[3].at.duration_since(calls[2].at);
    assert!(gap_429 >= Duration::from_millis(300), "429 gap {gap_429:?}");
    assert!(gap_503 < Duration::from_millis(250), "503 gap {gap_503:?}");
}

/// Retry-After: 0 でも最低待ち (150ms) を守る。逆証明: 最低待ちを 0 にすると即 retry になる。
#[tokio::test]
async fn retry_after_zero_still_waits_min_wait() {
    let responder: Responder = Arc::new(|i, _| {
        if i == 0 {
            Resp::json(429, json!({})).header("retry-after", "0")
        } else {
            Resp::json(200, contact_json())
        }
    });
    let (base, fake) = spawn_fake(responder.clone()).await;
    let c = client(
        &base,
        ClientOptions {
            retry_base_delay: Duration::from_millis(1),
            rate_limited_min_wait: Duration::from_millis(400),
            ..fast_opts()
        },
    );
    c.get_object("contacts", "101", &[]).await.unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    let gap = calls[1].at.duration_since(calls[0].at);
    assert!(gap >= Duration::from_millis(400), "gap {gap:?}");

    // 逆証明: 最低待ち 0 なら 400ms も待たない (= 上の待ちは min_wait によるもの)
    let (base, fake) = spawn_fake(responder).await;
    let c = client(
        &base,
        ClientOptions {
            retry_base_delay: Duration::from_millis(1),
            rate_limited_min_wait: Duration::ZERO,
            ..fast_opts()
        },
    );
    c.get_object("contacts", "101", &[]).await.unwrap();
    let calls = fake.calls();
    let gap = calls[1].at.duration_since(calls[0].at);
    assert!(gap < Duration::from_millis(300), "gap {gap:?}");
}

#[test]
fn default_options_are_conservative_for_shared_key() {
    let o = ClientOptions::default();
    assert_eq!(o.search_min_interval, Duration::from_millis(1000));
    assert_eq!(o.rate_limited_min_wait, Duration::from_secs(1));
    assert_eq!(o.max_retries, 2);
    assert_eq!(o.timeout, Duration::from_secs(10));
}

// ---------------------------------------------------------------------------
// 3. 401 / 4. 500 / 5. タイムアウト / その他のステータス
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unauthorized_is_not_retried() {
    let (base, fake) = spawn_fake(always(401, json!({"message": "bad token"}))).await;
    let err = client(&base, fast_opts())
        .get_object("contacts", "101", &[])
        .await
        .unwrap_err();
    assert_eq!(err, HubSpotError::Auth { status: 401 });
    assert_eq!(fake.count(), 1);
}

#[tokio::test]
async fn server_error_retries_until_exhausted() {
    let (base, fake) = spawn_fake(always(500, json!({}))).await;
    let opts = fast_opts();
    let max = opts.max_retries as usize;
    let err = client(&base, opts)
        .get_object("deals", "7", &[])
        .await
        .unwrap_err();
    assert_eq!(err, HubSpotError::Upstream { status: 500 });
    assert_eq!(fake.count(), max + 1);
    assert_eq!(fake.count(), 3);
}

#[tokio::test]
async fn slow_server_times_out() {
    let responder: Responder =
        Arc::new(|_, _| Resp::json(200, contact_json()).delay(Duration::from_millis(600)));
    let (base, fake) = spawn_fake(responder).await;
    let c = client(
        &base,
        ClientOptions {
            timeout: Duration::from_millis(100),
            max_retries: 1,
            ..fast_opts()
        },
    );
    let err = c.get_object("contacts", "101", &[]).await.unwrap_err();
    assert_eq!(err, HubSpotError::Timeout);
    // タイムアウトも retry 対象 (1 + max_retries)
    assert_eq!(fake.count(), 2);
}

#[tokio::test]
async fn other_statuses_map_without_retry() {
    for (status, expected) in [
        (403, HubSpotError::Auth { status: 403 }),
        (404, HubSpotError::NotFound),
        (400, HubSpotError::Upstream { status: 400 }),
    ] {
        let (base, fake) = spawn_fake(always(status, json!({"message": "x"}))).await;
        let err = client(&base, fast_opts())
            .get_object("companies", "5", &[])
            .await
            .unwrap_err();
        assert_eq!(err, expected, "status {status}");
        assert_eq!(fake.count(), 1, "status {status}");
    }
}

// ---------------------------------------------------------------------------
// 6. associations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_associations_keeps_all_and_extracts_labels() {
    let body = json!({
        "results": [
            {"toObjectId": 55, "associationTypes": [
                {"category": "HUBSPOT_DEFINED", "typeId": 3, "label": null},
                {"category": "USER_DEFINED", "typeId": 12, "label": "意思決定者"}
            ]},
            {"toObjectId": 56, "associationTypes": [
                {"category": "HUBSPOT_DEFINED", "typeId": 3, "label": null}
            ]},
            {"toObjectId": 12345678901234_u64, "associationTypes": [
                {"category": "USER_DEFINED", "typeId": 14, "label": "担当"},
                {"category": "USER_DEFINED", "typeId": 15, "label": "紹介元"}
            ]}
        ],
        "paging": {"next": {"after": "MTAw", "link": "?after=MTAw"}}
    });
    let (base, fake) = spawn_fake(always(200, body)).await;
    let (refs, has_more) = client(&base, fast_opts())
        .list_associations("deals", "9001", "contacts")
        .await
        .unwrap();

    assert_eq!(refs.len(), 3);
    assert_eq!(refs[0].id, "55");
    assert_eq!(refs[0].labels, vec!["意思決定者".to_string()]);
    assert_eq!(refs[1].id, "56");
    assert!(refs[1].labels.is_empty());
    assert_eq!(refs[2].id, "12345678901234");
    assert_eq!(
        refs[2].labels,
        vec!["担当".to_string(), "紹介元".to_string()]
    );
    assert!(has_more);

    let calls = fake.calls();
    assert_eq!(
        calls[0].path,
        "/crm/v4/objects/deals/9001/associations/contacts"
    );
    assert_eq!(
        calls[0].query,
        vec![("limit".to_string(), "500".to_string())]
    );
}

#[tokio::test]
async fn list_associations_without_next_page() {
    let body = json!({"results": [{"toObjectId": 1, "associationTypes": []}]});
    let (base, _fake) = spawn_fake(always(200, body)).await;
    let (refs, has_more) = client(&base, fast_opts())
        .list_associations("contacts", "1", "calls")
        .await
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert!(!has_more);
}

// ---------------------------------------------------------------------------
// 7. batch read
// ---------------------------------------------------------------------------

#[tokio::test]
async fn batch_read_splits_into_chunks_of_100() {
    // 受け取った inputs をそのまま結果として返す
    let responder: Responder = Arc::new(|_, rec| {
        let results: Vec<Value> = rec.body["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| json!({"id": i["id"], "properties": {"dealname": format!("D{}", i["id"].as_str().unwrap())}}))
            .collect();
        Resp::json(200, json!({"status": "COMPLETE", "results": results}))
    });
    let (base, fake) = spawn_fake(responder).await;
    let ids: Vec<String> = (1..=150).map(|i| i.to_string()).collect();
    let recs = client(&base, fast_opts())
        .batch_read("deals", &ids, &["dealname", "dealstage"])
        .await
        .unwrap();

    assert_eq!(recs.len(), 150);
    assert_eq!(recs[0].id, "1");
    assert_eq!(recs[149].id, "150");
    assert_eq!(
        recs[149].properties.get("dealname"),
        Some(&Some("D150".to_string()))
    );

    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    for c in &calls {
        assert_eq!(c.method, "POST");
        assert_eq!(c.path, "/crm/v3/objects/deals/batch/read");
        assert_eq!(c.body["properties"], json!(["dealname", "dealstage"]));
    }
    assert_eq!(calls[0].body["inputs"].as_array().unwrap().len(), 100);
    assert_eq!(calls[1].body["inputs"].as_array().unwrap().len(), 50);
    assert_eq!(calls[0].body["inputs"][0], json!({"id": "1"}));
    assert_eq!(calls[1].body["inputs"][0], json!({"id": "101"}));
}

#[tokio::test]
async fn batch_read_empty_sends_nothing() {
    let (base, fake) = spawn_fake(always(200, json!({"results": []}))).await;
    let recs = client(&base, fast_opts())
        .batch_read("deals", &[], &["dealname"])
        .await
        .unwrap();
    assert!(recs.is_empty());
    assert_eq!(fake.count(), 0);
}

// ---------------------------------------------------------------------------
// 8. rate limit ヘッダ
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rate_limit_headers_are_recorded() {
    let responder: Responder = Arc::new(|_, _| {
        Resp::json(200, contact_json())
            .header("X-HubSpot-RateLimit-Max", "190")
            .header("X-HubSpot-RateLimit-Remaining", "187")
            .header("X-HubSpot-RateLimit-Daily-Remaining", "649000")
    });
    let (base, _fake) = spawn_fake(responder).await;
    let c = client(&base, fast_opts());
    assert_eq!(c.last_rate_limit(), None);
    c.get_object("contacts", "101", &[]).await.unwrap();
    assert_eq!(
        c.last_rate_limit(),
        Some(RateLimitSnapshot {
            max: Some(190),
            remaining: Some(187),
            daily_remaining: Some(649000),
        })
    );
}

// ---------------------------------------------------------------------------
// 9. トークン非露出 (エラー Display/Debug、client Debug、tracing 出力)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct LogBuf(Arc<Mutex<Vec<u8>>>);

impl Write for LogBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn token_never_appears_in_errors_debug_or_logs() {
    let logs = LogBuf::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    // 応答本文にトークンや入力値を混ぜても、エラーには載らないこと
    let echo = json!({"message": "Authorization: Bearer test-token-XYZ"});
    let responder: Responder = Arc::new(move |_, rec| {
        let status: u16 = rec.path.rsplit('/').next().unwrap().parse().unwrap();
        match status {
            // 成功だが壊れた JSON (Decode) / rate limit 残り少 (warn)
            200 => Resp {
                status: 200,
                headers: vec![
                    ("x-hubspot-ratelimit-max", "100".into()),
                    ("x-hubspot-ratelimit-remaining", "3".into()),
                ],
                body: "not json test-token-XYZ".into(),
                delay: Duration::ZERO,
            },
            999 => Resp::json(200, contact_json()).delay(Duration::from_millis(600)),
            s => Resp::json(s, echo.clone()),
        }
    });
    let (base, _fake) = spawn_fake(responder).await;
    let c = client(
        &base,
        ClientOptions {
            timeout: Duration::from_millis(100),
            max_retries: 1,
            ..fast_opts()
        },
    );

    let mut errors = Vec::new();
    for id in ["401", "403", "404", "400", "429", "500", "200", "999"] {
        // 並列に走る他テストが購読者なしで先に callsite を登録すると interest が
        // never のまま残ることがあるため (tracing の既知の競合)、毎回作り直す
        tracing::callsite::rebuild_interest_cache();
        errors.push(c.get_object("contacts", id, &["email"]).await.unwrap_err());
    }
    errors.push(c.get_object("contacts", "1/2", &[]).await.unwrap_err());
    errors.push(c.get_object("../x", "1", &[]).await.unwrap_err());
    // 接続失敗: bind して local_addr を得てから listener を閉じたアドレス。
    // OS が直後に別プロセスへ同じポートを割り当てる可能性は 0 ではないため、
    // 判定は「Transport または Timeout」のどちらでもよい形にする (下の kinds 参照)。
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        drop(l);
        format!("http://{a}")
    };
    // Windows は拒否まで SYN を ~2 秒再送するので、タイムアウトより先に拒否が来るよう長めにする
    let c2 = client(
        &closed,
        ClientOptions {
            timeout: Duration::from_secs(10),
            max_retries: 0,
            ..fast_opts()
        },
    );
    let conn_err = c2.get_object("contacts", "1", &[]).await.unwrap_err();
    assert!(
        matches!(conn_err, HubSpotError::Transport(_) | HubSpotError::Timeout),
        "{conn_err:?}"
    );
    errors.push(conn_err);

    let kinds: Vec<&str> = errors.iter().map(|e| e.error_kind()).collect();
    let (conn_kind, kinds) = kinds.split_last().unwrap();
    assert!(
        ["hubspot_transport", "hubspot_timeout"].contains(conn_kind),
        "{conn_kind}"
    );
    assert_eq!(
        kinds,
        vec![
            "hubspot_auth",
            "hubspot_auth",
            "not_found",
            "hubspot_upstream",
            "hubspot_rate_limited",
            "hubspot_upstream",
            "hubspot_decode",
            "hubspot_timeout",
            "hubspot_decode",
            "hubspot_decode",
        ]
    );

    for e in &errors {
        let shown = format!("{e} / {e:?}");
        assert!(!shown.contains(TOKEN), "token leaked in error: {shown}");
        assert!(!shown.contains("127.0.0.1"), "url leaked in error: {shown}");
    }
    for c in [&c, &c2] {
        let dbg = format!("{c:?}");
        assert!(!dbg.contains(TOKEN), "token leaked in Debug: {dbg}");
    }

    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    // 捕捉が効いている証拠: rate limit 残少の warn と retry の warn が出ている
    assert!(
        text.contains("レート制限の残りが 10% 未満") && text.contains("remaining=3"),
        "{text}"
    );
    assert!(text.contains("HubSpot API を retry します"), "{text}");
    assert!(!text.contains(TOKEN), "token leaked in logs");
}

// ---------------------------------------------------------------------------
// 本体 + 関連を 1 回で取る / v4 batch associations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_object_with_associations_is_one_request() {
    let mut body = contact_json();
    body["associations"] = json!({
        "companies": {"results": [{"id": "300", "type": "contact_to_company"}]},
        "calls": {
            "results": [
                {"id": "1001", "type": "contact_to_call"},
                {"id": "1001", "type": "contact_to_call_unlabeled"},
                {"id": "1002", "type": "contact_to_call"}
            ],
            "paging": {"next": {"after": "2"}}
        }
    });
    let (base, fake) = spawn_fake(always(200, body)).await;
    let (rec, assocs) = client(&base, fast_opts())
        .get_object_with_associations(
            "contacts",
            "101",
            &["email"],
            &["companies", "calls", "notes"],
        )
        .await
        .unwrap();
    assert_eq!(rec.id, "101");
    assert_eq!(
        rec.properties["email"].as_deref(),
        Some("yamada@example.com")
    );
    // 1 回だけ。associations は要求した型をカンマ連結
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
    assert_eq!(calls[0].path, "/crm/v3/objects/contacts/101");
    assert!(calls[0].query.contains(&(
        "associations".to_string(),
        "companies,calls,notes".to_string()
    )));
    assert!(calls[0]
        .query
        .contains(&("properties".to_string(), "email".to_string())));

    let (companies, more) = &assocs["companies"];
    assert_eq!(companies.len(), 1);
    assert_eq!(companies[0].id, "300");
    assert!(companies[0].labels.is_empty());
    assert_eq!(companies[0].type_names, vec!["contact_to_company"]);
    assert!(!more);
    // 同じ id の重複は 1 件に、paging.next があれば truncated
    let (calls_refs, more) = &assocs["calls"];
    assert_eq!(
        calls_refs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["1001", "1002"]
    );
    assert!(more);
    // 同じ id の型名はまとめて持つ (ラベルは呼び出し側が定義から引く)
    assert_eq!(
        calls_refs[0].type_names,
        vec!["contact_to_call", "contact_to_call_unlabeled"]
    );
    assert_eq!(calls_refs[1].type_names, vec!["contact_to_call"]);
    // 応答に無い型も空で入っている
    assert_eq!(assocs["notes"], (Vec::new(), false));
}

#[tokio::test]
async fn association_labels_reads_definitions() {
    let body = json!({"results": [
        {"category": "HUBSPOT_DEFINED", "typeId": 341, "label": null},
        {"category": "HUBSPOT_DEFINED", "typeId": 5, "label": "Primary"},
        {"category": "USER_DEFINED", "typeId": 17, "label": "主"},
        {"category": "USER_DEFINED", "label": "id なし"}
    ]});
    let (base, fake) = spawn_fake(always(200, body)).await;
    let defs = client(&base, fast_opts())
        .association_labels("deals", "companies")
        .await
        .unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
    assert_eq!(calls[0].path, "/crm/v4/associations/deals/companies/labels");
    assert_eq!(
        defs.iter()
            .map(|d| (d.category.as_str(), d.type_id, d.label.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            ("HUBSPOT_DEFINED", 341, None),
            ("HUBSPOT_DEFINED", 5, Some("Primary")),
            ("USER_DEFINED", 17, Some("主")),
        ]
    );
    // 不正な型名はパスに入れない
    let err = client(&base, fast_opts())
        .association_labels("deals", "../x")
        .await
        .unwrap_err();
    assert_eq!(err.error_kind(), "hubspot_decode");
    assert_eq!(fake.count(), 1);
}

#[tokio::test]
async fn get_object_with_associations_rejects_bad_type() {
    let (base, fake) = spawn_fake(always(200, contact_json())).await;
    let err = client(&base, fast_opts())
        .get_object_with_associations("contacts", "101", &[], &["../x"])
        .await
        .unwrap_err();
    assert_eq!(err.error_kind(), "hubspot_decode");
    assert_eq!(fake.count(), 0);
}

#[tokio::test]
async fn batch_associations_splits_by_100_and_maps_from_ids() {
    // 受け取った inputs の各 id について、to を 1 件 (id + 10000) 返す。id "5" だけ関連なし
    let responder: Responder = Arc::new(|_, rec| {
        let results: Vec<Value> = rec.body["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["id"] != "5")
            .map(|i| {
                let n: u64 = i["id"].as_str().unwrap().parse().unwrap();
                json!({"from": {"id": i["id"]},
                       "to": [{"toObjectId": n + 10000, "associationTypes": [
                           {"category": "HUBSPOT_DEFINED", "typeId": 194, "label": null}]}]})
            })
            .collect();
        Resp::json(200, json!({"status": "COMPLETE", "results": results}))
    });
    let (base, fake) = spawn_fake(responder).await;
    let ids: Vec<String> = (1..=150).map(|i| i.to_string()).collect();
    let out = client(&base, fast_opts())
        .batch_associations("contacts", "calls", &ids)
        .await
        .unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].method, "POST");
    assert_eq!(
        calls[0].path,
        "/crm/v4/associations/contacts/calls/batch/read"
    );
    assert_eq!(calls[0].body["inputs"].as_array().unwrap().len(), 100);
    assert_eq!(calls[1].body["inputs"].as_array().unwrap().len(), 50);
    assert_eq!(out.len(), 149);
    assert_eq!(out["1"][0].id, "10001");
    assert_eq!(out["150"][0].id, "10150");
    assert!(!out.contains_key("5"));

    // 空なら送らない
    let n = fake.count();
    assert!(client(&base, fast_opts())
        .batch_associations("contacts", "calls", &[])
        .await
        .unwrap()
        .is_empty());
    assert_eq!(fake.count(), n);
}

// ---------------------------------------------------------------------------
// 10. Search の間隔
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_calls_are_spaced() {
    let (base, fake) = spawn_fake(always(200, json!({"total": 0, "results": []}))).await;
    let opts = fast_opts();
    let interval = opts.search_min_interval;
    let c = client(&base, opts);
    let body = json!({"filterGroups": [], "limit": 10});
    // 先に 1 本送って接続を張っておく (初回だけ TCP 接続の時間が到着時刻に乗り、
    // 到着間隔が見かけ上縮むため。2026-09-30 に初回込みの計測で 1 回落ちた)
    c.search("calls", body.clone()).await.unwrap();
    // 同時に 3 本投げても開始は間隔を空ける (本文が同じだと 1 回にまとめられるので、limit を変えて別の検索にする)
    let body_n = |n: u32| json!({"filterGroups": [], "limit": n});
    let (a, b, d) = tokio::join!(
        c.search("calls", body_n(11)),
        c.search("calls", body_n(12)),
        c.search("calls", body_n(13)),
    );
    assert_eq!(a.unwrap()["total"], 0);
    b.unwrap();
    d.unwrap();

    let calls = fake.calls();
    assert_eq!(calls.len(), 4);
    assert!(calls
        .iter()
        .all(|c| c.method == "POST" && c.path == "/crm/v3/objects/calls/search"));
    assert_eq!(calls[0].body, body);
    let spread = calls[3].at.duration_since(calls[1].at);
    assert!(spread >= interval * 2, "spread {spread:?}");
}

// ---------------------------------------------------------------------------
// 11. パス注入の防止
// ---------------------------------------------------------------------------

#[tokio::test]
async fn invalid_id_or_object_never_reaches_server() {
    let (base, fake) = spawn_fake(always(200, contact_json())).await;
    let c = client(&base, fast_opts());
    for id in ["1/2", "abc", "", "123456789012345678901", "12 3", "１２"] {
        let err = c.get_object("contacts", id, &[]).await.unwrap_err();
        assert_eq!(err, HubSpotError::Decode("invalid id".into()), "id {id:?}");
        let err = c
            .list_associations("deals", id, "contacts")
            .await
            .unwrap_err();
        assert_eq!(err, HubSpotError::Decode("invalid id".into()), "id {id:?}");
    }
    let err = c
        .batch_read("deals", &["1".into(), "../2".into()], &[])
        .await
        .unwrap_err();
    assert_eq!(err, HubSpotError::Decode("invalid id".into()));
    for object in ["contacts/1", "tickets", "", "..", "Contacts"] {
        let err = c.get_object(object, "1", &[]).await.unwrap_err();
        assert_eq!(err, HubSpotError::Decode("invalid object".into()));
        let err = c.search(object, json!({})).await.unwrap_err();
        assert_eq!(err, HubSpotError::Decode("invalid object".into()));
    }
    let err = c
        .list_associations("deals", "1", "x/../contacts")
        .await
        .unwrap_err();
    assert_eq!(err, HubSpotError::Decode("invalid object".into()));
    assert_eq!(fake.count(), 0);

    // 20 桁ちょうどは通る (境界)
    c.get_object("contacts", "12345678901234567890", &[])
        .await
        .unwrap();
    assert_eq!(fake.count(), 1);
}

// ---------------------------------------------------------------------------
// 段階 A (逆証明): HubSpot の応答の端
// ---------------------------------------------------------------------------

/// 関連の一部の形が崩れていても (id 無し / results が null / キーごと null) 本体は読める。
/// 崩れた関連 1 件でレコード全体を 502 にしない。
#[tokio::test]
async fn associations_with_malformed_entries_are_skipped_not_fatal() {
    let mut body = contact_json();
    body["associations"] = json!({
        "companies": {"results": [{"type": "no_id"}, {"id": "300", "type": "ok"}]},
        "calls": {"paging": {}},
        "notes": null,
        "tasks": {"results": null}
    });
    let (base, _fake) = spawn_fake(always(200, body)).await;
    let (rec, assocs) = client(&base, fast_opts())
        .get_object_with_associations(
            "contacts",
            "101",
            &[],
            &["companies", "calls", "notes", "tasks"],
        )
        .await
        .expect("崩れた関連で本体まで失敗してはいけない");
    assert_eq!(rec.id, "101");
    let ids = |k: &str| {
        assocs[k]
            .0
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("companies"), vec!["300"]);
    assert!(ids("calls").is_empty());
    assert!(ids("notes").is_empty());
    assert!(ids("tasks").is_empty());
}

/// batch read で全 ID が見つからないとき、HubSpot は 207 で `results` 無し・`errors` だけを返しうる。
/// 「応答を解釈できない」ではなく 0 件として扱う。
#[tokio::test]
async fn batch_read_with_only_errors_is_empty_not_decode_error() {
    let body = json!({"status": "COMPLETE", "numErrors": 2,
                      "errors": [{"status": "error", "category": "OBJECT_NOT_FOUND"}]});
    let (base, _fake) = spawn_fake(always(207, body)).await;
    let recs = client(&base, fast_opts())
        .batch_read(
            "calls",
            &["1".to_string(), "2".to_string()],
            &["hs_timestamp"],
        )
        .await
        .expect("results が無くても 0 件");
    assert!(recs.is_empty());
}

/// v4 の batch associations も同じ (関連が 1 件も無い from だけを渡したとき)
#[tokio::test]
async fn batch_associations_with_only_errors_is_empty_not_decode_error() {
    let body = json!({"status": "COMPLETE", "numErrors": 1,
                      "errors": [{"status": "error", "category": "NO_ASSOCIATIONS_FOUND"}]});
    let (base, _fake) = spawn_fake(always(207, body)).await;
    let map = client(&base, fast_opts())
        .batch_associations("contacts", "calls", &["1".to_string()])
        .await
        .expect("results が無くても空");
    assert!(map.is_empty());
}

/// 3xx は辿らない (Bearer を別ホストへ持ち出さない)。1 回だけ送って Upstream エラー。
#[tokio::test]
async fn redirects_are_not_followed() {
    let responder: Responder = Arc::new(|_, _| {
        Resp::json(302, json!({})).header("location", "http://127.0.0.1:1/elsewhere")
    });
    let (base, fake) = spawn_fake(responder).await;
    let err = client(&base, fast_opts())
        .get_object("contacts", "101", &[])
        .await
        .unwrap_err();
    assert_eq!(err, HubSpotError::Upstream { status: 302 });
    assert_eq!(fake.count(), 1);
}

/// 応答の `properties` が無い / null でも、レコードは空のプロパティで読める
#[tokio::test]
async fn record_without_properties_reads_as_empty() {
    for body in [
        json!({"id": "7", "createdAt": "2026-01-01T00:00:00Z"}),
        json!({"id": 7, "properties": null}),
    ] {
        let (base, _fake) = spawn_fake(always(200, body)).await;
        let rec = client(&base, fast_opts())
            .get_object("deals", "7", &["dealname"])
            .await
            .unwrap();
        assert_eq!(rec.id, "7");
        assert!(rec.properties.is_empty());
        assert!(!rec.archived);
    }
}

// ---------------------------------------------------------------------------
// 関所 (gateway) を通した振る舞い: 相乗り・429 の全員停止・待ちの上限・生の読み取り
// ---------------------------------------------------------------------------

/// 同じ読み取りが 50 本同時に来ても HubSpot への呼び出しは 1 回で、全員が同じ結果を受け取る
#[tokio::test]
async fn identical_concurrent_reads_are_coalesced_into_one_call() {
    let responder: Responder =
        Arc::new(|_, _| Resp::json(200, contact_json()).delay(Duration::from_millis(200)));
    let (base, fake) = spawn_fake(responder).await;
    let c = client(&base, fast_opts());
    let mut tasks = Vec::new();
    for _ in 0..50 {
        let c = c.clone();
        tasks.push(tokio::spawn(async move {
            c.get_object("contacts", "101", &["firstname"]).await
        }));
    }
    for t in tasks {
        let rec = t.await.unwrap().unwrap();
        assert_eq!(rec.id, "101");
        assert_eq!(
            rec.properties.get("firstname"),
            Some(&Some("山田".to_string()))
        );
    }
    assert_eq!(fake.count(), 1, "50 本が 1 回にまとまる");
    let snap = c.gateway().snapshot();
    assert_eq!(snap.coalesced, 49);
    assert_eq!(snap.total_calls, 1);
    assert_eq!(snap.calls_by_group.get("object_read"), Some(&1));

    // 終わった読み取りは表に残らない (次の要求は新しく呼ぶ)
    c.get_object("contacts", "101", &["firstname"])
        .await
        .unwrap();
    assert_eq!(fake.count(), 2);
    // 中身が違う読み取り (項目・優先度) はまとめない
    let bg = c.background();
    let (a, b) = tokio::join!(
        c.get_object("contacts", "101", &["email"]),
        bg.get_object("contacts", "101", &["email"]),
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(fake.count(), 4);
}

/// 1 本が 429 を受けたら、同じ関所を使う別のクライアントの呼び出しも Retry-After の間止まる
#[tokio::test]
async fn one_429_pauses_other_callers_on_the_same_gateway() {
    let responder: Responder = Arc::new(|i, r| {
        if i == 0 && r.path == "/crm/v3/objects/deals/1" {
            Resp::json(429, json!({})).header("retry-after", "1")
        } else {
            Resp::json(200, json!({"id": "2", "properties": {}}))
        }
    });
    let (base, fake) = spawn_fake(responder).await;
    let gw = Arc::new(crate::hubspot::Gateway::new(
        crate::hubspot::GatewayConfig::unlimited(
            Duration::from_millis(1),
            Duration::from_millis(20),
        ),
    ));
    let opts = || ClientOptions {
        max_retries: 0,
        ..fast_opts()
    };
    let a = HubSpotClient::with_gateway(TOKEN.into(), &base, opts(), gw.clone()).unwrap();
    let b = HubSpotClient::with_gateway(TOKEN.into(), &base, opts(), gw.clone()).unwrap();
    assert_eq!(
        a.get_object("deals", "1", &[]).await.unwrap_err(),
        HubSpotError::RateLimited
    );
    // 別のクライアント・別のレコードでも、Retry-After (1 秒) が明けるまで送らない
    b.get_object("deals", "2", &[]).await.unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    let gap = calls[1].at.duration_since(calls[0].at);
    assert!(gap >= Duration::from_millis(950), "gap {gap:?}");
    assert_eq!(gw.snapshot().rate_limited, 1);
}

/// 待ちの上限を超えるなら HubSpot を呼ばずに Busy (503 hubspot_busy)
#[tokio::test]
async fn waiting_longer_than_the_deadline_fails_fast_with_busy() {
    let (base, fake) = spawn_fake(always(200, contact_json())).await;
    let mut cfg = crate::hubspot::GatewayConfig::unlimited(
        Duration::from_millis(1),
        Duration::from_millis(20),
    );
    cfg.interactive_max_wait = Duration::from_millis(300);
    let gw = Arc::new(crate::hubspot::Gateway::new(cfg));
    let c = HubSpotClient::with_gateway(TOKEN.into(), &base, fast_opts(), gw.clone()).unwrap();
    gw.on_rate_limited(Some(Duration::from_secs(5)));
    let started = Instant::now();
    let err = c.get_object("contacts", "101", &[]).await.unwrap_err();
    assert_eq!(err, HubSpotError::Busy);
    assert_eq!(err.error_kind(), "hubspot_busy");
    assert_eq!(err.http_status(), 503);
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "並ばずに断る"
    );
    assert_eq!(fake.count(), 0, "HubSpot には届かない");
    assert_eq!(gw.snapshot().busy, 1);
}

/// 生の読み取りは失敗の status を返し (retry しない)、パスは /crm/ の安全な形だけ
#[tokio::test]
async fn read_raw_returns_status_without_retry_and_rejects_unsafe_paths() {
    let responder: Responder = Arc::new(|i, _| {
        if i == 0 {
            Resp::json(429, json!({})).header("retry-after", "2")
        } else {
            Resp::json(200, json!({"results": []}))
        }
    });
    let (base, fake) = spawn_fake(responder).await;
    let c = client(&base, fast_opts());
    let r = c
        .read_raw(
            false,
            "/crm/v3/objects/0-420/1",
            &[("properties", "a".into())],
            None,
        )
        .await
        .unwrap();
    assert_eq!(r.status, 429);
    assert_eq!(r.retry_after_secs, Some(2));
    assert_eq!(r.body, None);
    assert_eq!(fake.count(), 1, "retry しない");
    let r = c
        .read_raw(
            true,
            "/crm/v3/objects/0-420/batch/read",
            &[],
            Some(&json!({"inputs": []})),
        )
        .await
        .unwrap();
    assert_eq!(r.status, 200);
    assert_eq!(r.body, Some(json!({"results": []})));
    for bad in [
        "/oauth/v1/x",
        "/crm/v3/../oauth",
        "/crm/v3//x",
        "/crm/v3/objects/1?x=1",
        "http://evil/crm/v3",
    ] {
        assert!(
            matches!(
                c.read_raw(false, bad, &[], None).await,
                Err(HubSpotError::Decode(_))
            ),
            "{bad}"
        );
    }
    assert_eq!(fake.count(), 2);
}

/// 応答の X-HubSpot-RateLimit-* は関所にも残る (管理画面の表示用)
#[tokio::test]
async fn rate_limit_headers_are_kept_on_the_gateway() {
    let responder: Responder = Arc::new(|_, _| {
        Resp::json(200, contact_json())
            .header("x-hubspot-ratelimit-max", "190")
            .header("x-hubspot-ratelimit-remaining", "180")
            .header("x-hubspot-ratelimit-secondly", "19")
            .header("x-hubspot-ratelimit-secondly-remaining", "18")
            .header("x-hubspot-ratelimit-daily", "625000")
            .header("x-hubspot-ratelimit-daily-remaining", "624000")
    });
    let (base, _fake) = spawn_fake(responder).await;
    let c = client(&base, fast_opts());
    c.get_object("contacts", "101", &[]).await.unwrap();
    let r = c.gateway().snapshot().rate_limit;
    assert_eq!(
        (
            r.max,
            r.remaining,
            r.secondly,
            r.secondly_remaining,
            r.daily,
            r.daily_remaining
        ),
        (
            Some(190),
            Some(180),
            Some(19),
            Some(18),
            Some(625_000),
            Some(624_000)
        )
    );
}
