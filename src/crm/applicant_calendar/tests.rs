//! 応募の日付をカレンダーに載せる機能のテスト。HubSpot・Google カレンダーは偽物だけを使う。
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde_json::{json, Value};

use super::config::{parse_props, CalendarConfig};
use super::google::{self, aad, CalError, CalendarApi, Inserted, Problem, SCOPE};
use super::routes::consent_allowed;
use super::sync::{date_search_bodies, CalendarHubSpot, Cycle, Outcome, Params};
use super::*;
use crate::audit::fake_turso::start_sqlite_audit;
use crate::config::GoogleOidcConfig;
use crate::crm::email::{grants, EmailConfig, EmailService};
use crate::crm::pending;
use crate::hubspot::HubSpotRecord;

const PORTAL: &str = "1234";
const GUEST: &str = "consultant@example.com";
const ACCOUNT: &str = "calendar-admin@example.com";
/// 2026-11-01 00:00 UTC (= 09:00 JST)
const NOW_MS: i64 = 1_793_491_200_000;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(NOW_MS).unwrap()
}

fn cfg() -> CalendarConfig {
    CalendarConfig::from_lookup(|_| None)
}

fn params() -> Params {
    Params {
        pacing: Duration::ZERO,
        reap_every: Duration::from_secs(3600),
    }
}

// ---------------------------------------------------------------------------
// 純粋な関数
// ---------------------------------------------------------------------------

#[test]
fn now_constant_is_2026_11_01() {
    assert_eq!(now(), Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap());
}

#[test]
fn event_id_is_deterministic_base32hex() {
    let a = event_id("101", "mensetsuyoteibi");
    assert_eq!(a, event_id("101", "mensetsuyoteibi"));
    assert_ne!(a, event_id("101", "nyuushayoteibi"));
    assert_ne!(a, event_id("102", "mensetsuyoteibi"));
    assert_eq!(a.len(), 36);
    assert!(a.starts_with("hrhr"));
    assert!(a
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'v').contains(&b)));
}

#[test]
fn when_follows_hubspot_type() {
    // 日時型: UTC の ISO / ミリ秒
    let t = Utc.with_ymd_and_hms(2026, 11, 5, 1, 30, 0).unwrap();
    assert_eq!(
        when_of("datetime", "2026-11-05T01:30:00.000Z"),
        Some(When::Timed(t))
    );
    assert_eq!(
        when_of("datetime", &t.timestamp_millis().to_string()),
        Some(When::Timed(t))
    );
    // 日付型: 終日 (HubSpot は UTC の 0 時で持つ)
    let d = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
    assert_eq!(when_of("date", "2026-11-05"), Some(When::AllDay(d)));
    let midnight = Utc.with_ymd_and_hms(2026, 11, 5, 0, 0, 0).unwrap();
    assert_eq!(
        when_of("date", &midnight.timestamp_millis().to_string()),
        Some(When::AllDay(d))
    );
    // 型が日時・日付でない・空・読めない
    assert_eq!(when_of("string", "2026-11-05"), None);
    assert_eq!(when_of("date", ""), None);
    assert_eq!(when_of("datetime", "あした"), None);
}

#[test]
fn summary_uses_fullwidth_bar_and_skips_blanks() {
    assert_eq!(
        summary("面接", "山田 太郎", "テスト商事"),
        "面接｜山田 太郎｜テスト商事"
    );
    assert_eq!(summary("入社", "山田 太郎", ""), "入社｜山田 太郎");
    assert_eq!(summary("内定", "", ""), "内定");
}

#[test]
fn bodies_switch_between_timed_and_all_day() {
    let t = Utc.with_ymd_and_hms(2026, 11, 5, 1, 30, 0).unwrap();
    let mut d = Desired {
        applicant_id: "101".into(),
        prop: "mensetsuyoteibi".into(),
        summary: "面接｜A".into(),
        description: "x".into(),
        when: When::Timed(t),
        attendees: vec![GUEST.into()],
    };
    let p = patch_body(&d, 60);
    assert_eq!(p["start"]["dateTime"], "2026-11-05T10:30:00+09:00");
    assert_eq!(p["end"]["dateTime"], "2026-11-05T11:30:00+09:00");
    assert_eq!(p["start"]["timeZone"], "Asia/Tokyo");
    // 終日から時刻つきへ切り替えるとき、古い date を消す
    assert!(p["start"].get("date").is_some_and(Value::is_null));
    assert_eq!(p["attendees"], json!([{"email": GUEST}]));
    assert_eq!(p["status"], "confirmed");
    let i = insert_body(&d, 60);
    assert_eq!(i["id"], event_id("101", "mensetsuyoteibi"));
    assert!(
        i["start"].get("date").is_none(),
        "作成のときは null を送らない"
    );
    // 終日は end が翌日
    d.when = When::AllDay(NaiveDate::from_ymd_opt(2026, 11, 30).unwrap());
    let p = patch_body(&d, 60);
    assert_eq!(p["start"]["date"], "2026-11-30");
    assert_eq!(p["end"]["date"], "2026-12-01");
    assert!(p["start"].get("dateTime").is_some_and(Value::is_null));
}

#[test]
fn same_compares_instants_and_attendees() {
    let t = Utc.with_ymd_and_hms(2026, 11, 5, 1, 30, 0).unwrap();
    let d = Desired {
        applicant_id: "101".into(),
        prop: "mensetsuyoteibi".into(),
        summary: "面接｜A".into(),
        description: "x".into(),
        when: When::Timed(t),
        attendees: vec![GUEST.into()],
    };
    let mut e = insert_body(&d, 60);
    e["status"] = json!("confirmed");
    assert!(same(&e, &d, 60));
    // Google が別の書き方 (UTC) で返しても同じ時刻なら同じ
    e["start"]["dateTime"] = json!("2026-11-05T01:30:00Z");
    assert!(same(&e, &d, 60));
    // ゲストの大文字小文字は問わない
    e["attendees"] = json!([{"email": "Consultant@Example.com", "responseStatus": "needsAction"}]);
    assert!(same(&e, &d, 60));
    let mut moved = e.clone();
    moved["start"]["dateTime"] = json!("2026-11-05T11:00:00+09:00");
    assert!(!same(&moved, &d, 60));
    let mut no_guest = e.clone();
    no_guest["attendees"] = json!([]);
    assert!(!same(&no_guest, &d, 60));
    let mut cancelled = e.clone();
    cancelled["status"] = json!("cancelled");
    assert!(!same(&cancelled, &d, 60));
}

#[test]
fn props_config_parses_and_falls_back() {
    let c = cfg();
    assert_eq!(
        c.props,
        vec![
            ("mensetsuyoteibi".to_string(), "面接".to_string()),
            ("nyuushayoteibi".to_string(), "入社".to_string()),
            ("naiteinichi".to_string(), "内定".to_string()),
        ]
    );
    assert_eq!(
        (c.calendar_id.as_str(), c.event_minutes, c.enabled),
        ("primary", 60, false)
    );
    assert_eq!(c.interval, Duration::from_secs(300));
    assert_eq!(c.account, None);
    assert_eq!(
        parse_props(Some("a_1:説明会")),
        Some(vec![("a_1".into(), "説明会".into())])
    );
    assert_eq!(parse_props(Some("Bad-Name:面接")), None);
    let c = CalendarConfig::from_lookup(|k| {
        match k {
            "APPLICANT_CALENDAR_PROPS" => Some("x y:面接"),
            "APPLICANT_CALENDAR_ACCOUNT" => Some(" Calendar-Admin@Example.com "),
            "APPLICANT_CALENDAR_EVENT_MINUTES" => Some("30"),
            "APPLICANT_CALENDAR_ENABLED" => Some("1"),
            _ => None,
        }
        .map(String::from)
    });
    assert_eq!(c.props.len(), 3, "形が違えば既定の 3 項目");
    assert_eq!(c.account.as_deref(), Some(ACCOUNT));
    assert_eq!((c.event_minutes, c.enabled), (30, true));
}

#[test]
fn consent_only_for_the_configured_account_with_scope() {
    let scopes = format!("openid email {SCOPE}");
    assert!(consent_allowed(
        ACCOUNT,
        ACCOUNT,
        "Calendar-Admin@example.com",
        false,
        &scopes
    )
    .is_ok());
    // Google で選んだアカウントが違う
    assert!(consent_allowed(ACCOUNT, ACCOUNT, "other@example.com", false, &scopes).is_err());
    // 手続きを始めた人が違う
    assert!(consent_allowed(ACCOUNT, "other@example.com", ACCOUNT, false, &scopes).is_err());
    // HR_HR で無効
    assert!(consent_allowed(ACCOUNT, ACCOUNT, ACCOUNT, true, &scopes).is_err());
    // 範囲が選ばれていない
    assert!(consent_allowed(ACCOUNT, ACCOUNT, ACCOUNT, false, "openid email").is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn grant_is_stored_with_its_own_aad() {
    let (audit, _conn) = start_sqlite_audit().await;
    let key = [7u8; 32];
    let enc = grants::encrypt(&key, &aad(ACCOUNT), "refresh-value").unwrap();
    let e2 = enc.clone();
    pending::blocking(&audit, move |t| google::save(t, ACCOUNT, &e2, SCOPE))
        .await
        .unwrap()
        .unwrap();
    let row = pending::blocking(&audit, |t| google::get(t, ACCOUNT))
        .await
        .unwrap()
        .unwrap()
        .expect("保存した許可");
    assert_eq!(
        grants::decrypt(&key, &aad(ACCOUNT), &row.encrypted).unwrap(),
        "refresh-value"
    );
    // CRM (AAD = アドレスだけ)・顧客画面の送信元 (別の用途の文字) としては復号できない
    assert!(grants::decrypt(&key, ACCOUNT, &enc).is_err());
    assert!(grants::decrypt(&key, &crate::portal::mail_grant::aad(ACCOUNT), &enc).is_err());
    // 逆に、顧客画面の送信元の暗号文はカレンダーとしては復号できない
    let portal = grants::encrypt(&key, &crate::portal::mail_grant::aad(ACCOUNT), "x").unwrap();
    assert!(grants::decrypt(&key, &aad(ACCOUNT), &portal).is_err());
    // 取り消すと読めない
    pending::blocking(&audit, |t| google::revoke(t, ACCOUNT))
        .await
        .unwrap()
        .unwrap();
    assert!(pending::blocking(&audit, |t| google::get(t, ACCOUNT))
        .await
        .unwrap()
        .unwrap()
        .is_none());
    // 範囲が足りない行は無いものとして扱う
    pending::blocking(&audit, move |t| {
        google::save(t, ACCOUNT, &enc, "openid email")
    })
    .await
    .unwrap()
    .unwrap();
    assert!(pending::blocking(&audit, |t| google::get(t, ACCOUNT))
        .await
        .unwrap()
        .unwrap()
        .is_none());
}

// ---------------------------------------------------------------------------
// 偽の Google (token と Calendar の HTTP)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct GoogleState {
    token_calls: usize,
    token_invalid: bool,
    /// (メソッド, パス, クエリ)
    requests: Vec<(String, String, String)>,
    insert_status: u16,
}
type Shared = Arc<Mutex<GoogleState>>;

async fn token(State(g): State<Shared>) -> axum::response::Response {
    let mut g = g.lock().unwrap();
    g.token_calls += 1;
    if g.token_invalid {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    }
    Json(json!({"access_token": "access-1", "expires_in": 3600})).into_response()
}

async fn events_post(
    State(g): State<Shared>,
    RawQuery(q): RawQuery,
    Path(cal): Path<String>,
) -> axum::response::Response {
    let mut g = g.lock().unwrap();
    g.requests.push(("POST".into(), cal, q.unwrap_or_default()));
    let s = if g.insert_status == 0 {
        200
    } else {
        g.insert_status
    };
    (StatusCode::from_u16(s).unwrap(), Json(json!({}))).into_response()
}

async fn event_any(
    State(g): State<Shared>,
    method: axum::http::Method,
    RawQuery(q): RawQuery,
    Path((cal, id)): Path<(String, String)>,
) -> axum::response::Response {
    let mut g = g.lock().unwrap();
    g.requests.push((
        method.to_string(),
        format!("{cal}/{id}"),
        q.unwrap_or_default(),
    ));
    if method == axum::http::Method::GET {
        return (StatusCode::NOT_FOUND, Json(json!({}))).into_response();
    }
    if method == axum::http::Method::DELETE {
        return StatusCode::GONE.into_response();
    }
    Json(json!({})).into_response()
}

async fn google_server() -> (Shared, String) {
    let g: Shared = Arc::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/token", post(token))
        .route("/calendar/calendars/{cal}/events", post(events_post))
        .route(
            "/calendar/calendars/{cal}/events/{id}",
            get(event_any).patch(event_any).delete(event_any),
        )
        .with_state(g.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (g, base)
}

async fn service(base: &str, granted: bool) -> Arc<ApplicantCalendar> {
    let email = Arc::new(EmailService::new(EmailConfig {
        key: [9; 32],
        google: GoogleOidcConfig {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            redirect_url: "http://localhost/auth/google/callback".into(),
            hosted_domain: "example.com".into(),
        },
        redirect_url: format!("http://localhost{}", google::CALLBACK),
        base_url: "http://localhost".into(),
        authorization_url: format!("{base}/authorize"),
        token_url: format!("{base}/token"),
        revoke_url: format!("{base}/revoke"),
        gmail_url: format!("{base}/gmail"),
    }));
    let (audit, _conn) = start_sqlite_audit().await;
    if granted {
        let enc = grants::encrypt(&[9; 32], &aad(ACCOUNT), "refresh-value").unwrap();
        pending::blocking(&audit, move |t| google::save(t, ACCOUNT, &enc, SCOPE))
            .await
            .unwrap()
            .unwrap();
    }
    let cfg = CalendarConfig::from_lookup(|k| {
        (k == "APPLICANT_CALENDAR_ACCOUNT").then(|| ACCOUNT.to_string())
    });
    ApplicantCalendar::from_config(cfg, Some(email), Some(audit))
        .unwrap()
        .with_calendar_base(&format!("{base}/calendar"))
}

#[tokio::test(flavor = "multi_thread")]
async fn access_token_is_cached_and_expired_grant_is_reported() {
    let (g, base) = google_server().await;
    let svc = service(&base, true).await;
    assert_eq!(svc.access().await.unwrap(), "access-1");
    assert_eq!(svc.access().await.unwrap(), "access-1");
    assert_eq!(g.lock().unwrap().token_calls, 1, "期限内は取り直さない");
    assert!(!svc.expired());
    // 許可が切れた (invalid_grant)
    svc.forget_token().await;
    g.lock().unwrap().token_invalid = true;
    assert_eq!(svc.access().await, Err(Problem::Expired));
    assert!(svc.expired());
    // 許可し直すと切れの印は消える
    svc.granted().await;
    assert!(!svc.expired());
    // 許可が無い
    let none = service(&base, false).await;
    assert_eq!(none.access().await, Err(Problem::NoGrant));
}

#[tokio::test(flavor = "multi_thread")]
async fn calendar_writes_never_send_invitations() {
    let (g, base) = google_server().await;
    let svc = service(&base, true).await;
    let cal = svc.calendar("tok".into());
    assert_eq!(
        cal.insert(&json!({"id": "hrhrabc"})).await,
        Ok(Inserted::Created)
    );
    g.lock().unwrap().insert_status = 409;
    assert_eq!(
        cal.insert(&json!({"id": "hrhrabc"})).await,
        Ok(Inserted::Exists)
    );
    cal.patch("hrhrabc", &json!({})).await.unwrap();
    cal.delete("hrhrabc").await.unwrap();
    assert_eq!(cal.get("hrhrabc").await, Ok(None));
    g.lock().unwrap().insert_status = 401;
    assert_eq!(cal.insert(&json!({})).await, Err(CalError::Auth));
    let reqs = g.lock().unwrap().requests.clone();
    for (method, path, query) in &reqs {
        assert!(path.starts_with("primary"), "{path}");
        if method != "GET" {
            assert_eq!(query, "sendUpdates=none", "{method} {path}");
        }
    }
    assert_eq!(reqs.iter().filter(|r| r.0 != "GET").count(), 5);
}

// ---------------------------------------------------------------------------
// 偽の HubSpot と偽のカレンダーで周回を確かめる
// ---------------------------------------------------------------------------

fn rec(id: &str, props: &[(&str, &str)]) -> HubSpotRecord {
    HubSpotRecord {
        id: id.into(),
        properties: props
            .iter()
            .map(|(k, v)| (k.to_string(), Some(v.to_string())))
            .collect(),
        created_at: None,
        updated_at: None,
        archived: false,
    }
}

fn defs(interview_type: &str) -> Vec<Value> {
    vec![
        json!({"name": "mensetsuyoteibi", "type": interview_type, "fieldType": "date", "label": "面接予定日"}),
        json!({"name": "nyuushayoteibi", "type": "datetime", "fieldType": "date", "label": "入社予定日"}),
        json!({"name": "naiteinichi", "type": "date", "label": "内定日"}),
        json!({"name": "oubosaki_kyuujin_title", "type": "string", "label": "応募先求人タイトル"}),
    ]
}

type AssocKey = (String, String);

#[derive(Default)]
struct FakeHs {
    defs: Mutex<Vec<Value>>,
    applicants: Mutex<Vec<Value>>,
    assoc: Mutex<HashMap<AssocKey, BTreeMap<String, Vec<String>>>>,
    records: Mutex<HashMap<String, Vec<HubSpotRecord>>>,
    owners: HashMap<String, String>,
    searches: Mutex<Vec<Value>>,
}

fn ms_of(raw: &str) -> Option<i64> {
    match when_of("datetime", raw)? {
        When::Timed(t) => Some(t.timestamp_millis()),
        When::AllDay(_) => None,
    }
}

impl CalendarHubSpot for FakeHs {
    async fn applicant_properties(&self) -> Result<Vec<Value>, String> {
        Ok(self.defs.lock().unwrap().clone())
    }
    async fn search_applicants(&self, body: Value) -> Result<Value, String> {
        self.searches.lock().unwrap().push(body.clone());
        let all = self.applicants.lock().unwrap().clone();
        let results: Vec<Value> = all
            .into_iter()
            .filter(|a| {
                body["filterGroups"].as_array().unwrap().iter().any(|g| {
                    g["filters"].as_array().unwrap().iter().all(|f| {
                        let v = a["properties"][f["propertyName"].as_str().unwrap()]
                            .as_str()
                            .and_then(ms_of);
                        let want: i64 = f["value"].as_str().unwrap().parse().unwrap();
                        v.is_some_and(|v| v >= want)
                    })
                })
            })
            .collect();
        Ok(json!({ "results": results }))
    }
    async fn associations(
        &self,
        from: &str,
        to: &str,
        ids: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, String> {
        let all = self
            .assoc
            .lock()
            .unwrap()
            .get(&(from.to_string(), to.to_string()))
            .cloned()
            .unwrap_or_default();
        Ok(all.into_iter().filter(|(k, _)| ids.contains(k)).collect())
    }
    async fn read(
        &self,
        object: &str,
        ids: &[String],
        _props: &[&str],
    ) -> Result<Vec<HubSpotRecord>, String> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .get(object)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| ids.contains(&r.id))
            .collect())
    }
    async fn owner_emails(&self) -> Result<HashMap<String, String>, String> {
        Ok(self.owners.clone())
    }
}

/// 偽のカレンダー。削除は status=cancelled にして残す (Google と同じく ID は使い回せず 409 になる)
#[derive(Default)]
struct FakeCal {
    events: Mutex<BTreeMap<String, Value>>,
    calls: Mutex<Vec<String>>,
}

/// Google の patch と同じく、入れ子のオブジェクトは項目ごとに上書きし、null の項目は消す
fn merge(target: &mut Value, patch: &Value) {
    for (k, v) in patch.as_object().unwrap() {
        if v.is_object() && k != "extendedProperties" {
            let slot = target
                .as_object_mut()
                .unwrap()
                .entry(k.clone())
                .or_insert(json!({}));
            for (k2, v2) in v.as_object().unwrap() {
                if v2.is_null() {
                    slot.as_object_mut().unwrap().remove(k2);
                } else {
                    slot[k2] = v2.clone();
                }
            }
        } else {
            target[k] = v.clone();
        }
    }
}

impl FakeCal {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn event(&self, applicant: &str, prop: &str) -> Value {
        self.events.lock().unwrap()[&event_id(applicant, prop)].clone()
    }
    fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }
}

impl CalendarApi for FakeCal {
    async fn list_ours(&self, _time_min: DateTime<Utc>) -> Result<Vec<Value>, CalError> {
        self.calls.lock().unwrap().push("list".into());
        Ok(self
            .events
            .lock()
            .unwrap()
            .values()
            .filter(|e| {
                e["status"] != "cancelled"
                    && e["extendedProperties"]["private"][MARK_KEY] == MARK_VALUE
            })
            .cloned()
            .collect())
    }
    async fn get(&self, id: &str) -> Result<Option<Value>, CalError> {
        self.calls.lock().unwrap().push(format!("get {id}"));
        Ok(self.events.lock().unwrap().get(id).cloned())
    }
    async fn insert(&self, body: &Value) -> Result<Inserted, CalError> {
        let id = body["id"].as_str().unwrap().to_string();
        self.calls.lock().unwrap().push(format!("insert {id}"));
        let mut events = self.events.lock().unwrap();
        if events.contains_key(&id) {
            return Ok(Inserted::Exists);
        }
        let mut e = body.clone();
        e["status"] = json!("confirmed");
        events.insert(id, e);
        Ok(Inserted::Created)
    }
    async fn patch(&self, id: &str, body: &Value) -> Result<(), CalError> {
        self.calls.lock().unwrap().push(format!("patch {id}"));
        let mut events = self.events.lock().unwrap();
        let e = events.get_mut(id).ok_or(CalError::Status(404))?;
        merge(e, body);
        Ok(())
    }
    async fn delete(&self, id: &str) -> Result<(), CalError> {
        self.calls.lock().unwrap().push(format!("delete {id}"));
        if let Some(e) = self.events.lock().unwrap().get_mut(id) {
            e["status"] = json!("cancelled");
        }
        Ok(())
    }
}

/// 10:30 JST
const INTERVIEW: &str = "2026-11-05T01:30:00.000Z";
const OFFER: &str = "2026-11-03";

fn applicant(id: &str, modified_ms: i64, props: &[(&str, &str)]) -> Value {
    let mut p = json!({
        "ouboshashimei": "山田 太郎",
        "oubosaki_torihiki_name": "取引名の会社",
        "oubosaki_kyuujin_title": "介護スタッフ",
        "hs_lastmodifieddate": modified_ms.to_string(),
    });
    for (k, v) in props {
        p[*k] = json!(v);
    }
    json!({"id": id, "properties": p})
}

/// 応募 101 → 求人票 501 → 取引 902 (報告シートなし・担当 O2) / 901 (報告シートあり・生きた主契約・担当 O1)。
/// 901 → 会社 701
fn hubspot() -> FakeHs {
    let hs = FakeHs {
        owners: HashMap::from([
            ("O1".to_string(), GUEST.to_string()),
            ("O2".to_string(), "other@example.com".to_string()),
            ("O3".to_string(), "ended@example.com".to_string()),
            ("O4".to_string(), "option@example.com".to_string()),
        ]),
        ..Default::default()
    };
    *hs.defs.lock().unwrap() = defs("datetime");
    *hs.applicants.lock().unwrap() = vec![applicant(
        "101",
        NOW_MS - 3_600_000,
        &[("mensetsuyoteibi", INTERVIEW), ("naiteinichi", OFFER)],
    )];
    {
        let mut assoc = hs.assoc.lock().unwrap();
        assoc.insert(
            ("0-421".into(), "0-420".into()),
            BTreeMap::from([("101".into(), vec!["501".into()])]),
        );
        assoc.insert(
            ("0-420".into(), "deals".into()),
            BTreeMap::from([("501".into(), vec!["902".into(), "901".into()])]),
        );
        assoc.insert(
            ("deals".into(), "companies".into()),
            BTreeMap::from([("901".into(), vec!["701".into()])]),
        );
    }
    let mut records = hs.records.lock().unwrap();
    records.insert(
        "deals".into(),
        vec![
            rec(
                "901",
                &[
                    (
                        "customer_sheet_url",
                        "https://docs.google.com/spreadsheets/d/x",
                    ),
                    ("dealstage", "52016153"),
                    ("dealname", "主契約"),
                    ("hubspot_owner_id", "O1"),
                ],
            ),
            rec(
                "902",
                &[
                    ("dealstage", "52016153"),
                    ("dealname", "シートなし"),
                    ("hubspot_owner_id", "O2"),
                ],
            ),
        ],
    );
    records.insert(
        "companies".into(),
        vec![rec("701", &[("name", "テスト商事株式会社")])],
    );
    records.insert("0-421".into(), vec![rec("101", &[])]);
    drop(records);
    hs
}

async fn run(cycle: &mut Cycle, hs: &FakeHs, cal: &FakeCal, at: DateTime<Utc>) -> Outcome {
    cycle.run_once(hs, cal, &cfg(), &params(), at, PORTAL).await
}

#[tokio::test]
async fn first_cycle_creates_timed_and_all_day_events_with_guest() {
    let (hs, cal) = (hubspot(), FakeCal::default());
    let mut c = Cycle::default();
    let o = run(&mut c, &hs, &cal, now()).await;
    assert_eq!(o.problem, None);
    assert_eq!(
        (o.counts.created, o.counts.updated, o.counts.errors),
        (2, 0, 0)
    );

    // 初回の読み方: 項目ごとの filterGroups の OR、値が今日 (JST) − 30 日の 0 時以降
    let s = hs.searches.lock().unwrap()[0].clone();
    let groups = s["filterGroups"].as_array().unwrap();
    assert_eq!(groups.len(), 3);
    // 2026-10-02 0:00 JST
    let from = Utc
        .with_ymd_and_hms(2026, 10, 1, 15, 0, 0)
        .unwrap()
        .timestamp_millis();
    assert_eq!(
        groups[0]["filters"][0],
        json!({"propertyName": "mensetsuyoteibi", "operator": "GTE", "value": from.to_string()})
    );

    let interview = cal.event("101", "mensetsuyoteibi");
    assert_eq!(interview["summary"], "面接｜山田 太郎｜テスト商事株式会社");
    assert_eq!(
        interview["start"],
        json!({"dateTime": "2026-11-05T10:30:00+09:00", "timeZone": "Asia/Tokyo"})
    );
    assert_eq!(interview["end"]["dateTime"], "2026-11-05T11:30:00+09:00");
    // 報告シートの取引の選び方 (シートのある 901) の担当者
    assert_eq!(interview["attendees"], json!([{ "email": GUEST }]));
    assert_eq!(
        interview["extendedProperties"]["private"],
        json!({"hrhr": "applicant", "hrhr_applicant": "101", "hrhr_prop": "mensetsuyoteibi"})
    );
    let desc = interview["description"].as_str().unwrap();
    assert!(desc.contains("https://app.hubspot.com/contacts/1234/record/0-421/101/"));
    assert!(desc.contains("求人: 介護スタッフ"));

    let offer = cal.event("101", "naiteinichi");
    assert_eq!(offer["summary"], "内定｜山田 太郎｜テスト商事株式会社");
    assert_eq!(offer["start"], json!({"date": "2026-11-03"}));
    assert_eq!(offer["end"], json!({"date": "2026-11-04"}));
    // 入社予定日は空なので予定を作らない
    assert!(!cal
        .events
        .lock()
        .unwrap()
        .contains_key(&event_id("101", "nyuushayoteibi")));
}

#[tokio::test]
async fn same_content_is_not_written_again() {
    let (hs, cal) = (hubspot(), FakeCal::default());
    let mut c = Cycle::default();
    run(&mut c, &hs, &cal, now()).await;
    cal.clear_calls();
    // 応募が更新された (日付は同じ) → 読むが書かない
    hs.applicants.lock().unwrap()[0]["properties"]["hs_lastmodifieddate"] =
        json!((NOW_MS + 60_000).to_string());
    let o = run(&mut c, &hs, &cal, now() + chrono::Duration::minutes(5)).await;
    assert_eq!(
        (o.counts.unchanged, o.counts.created, o.counts.updated),
        (2, 0, 0)
    );
    assert_eq!(cal.calls(), vec!["list"], "一覧を 1 回読むだけで書かない");
    // 2 回目以降の読み方: 前回の読み始め − 5 分以降に更新された応募
    let s = hs.searches.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        s["filterGroups"][0]["filters"][0],
        json!({"propertyName": "hs_lastmodifieddate", "operator": "GTE", "value": (NOW_MS - 300_000).to_string()})
    );
}

#[tokio::test]
async fn moved_value_patches_and_cleared_value_deletes() {
    let (hs, cal) = (hubspot(), FakeCal::default());
    let mut c = Cycle::default();
    run(&mut c, &hs, &cal, now()).await;
    {
        let mut a = hs.applicants.lock().unwrap();
        let p = &mut a[0]["properties"];
        p["mensetsuyoteibi"] = json!("2026-11-06T05:00:00Z");
        p["naiteinichi"] = json!("");
        p["hs_lastmodifieddate"] = json!((NOW_MS + 60_000).to_string());
    }
    cal.clear_calls();
    let o = run(&mut c, &hs, &cal, now() + chrono::Duration::minutes(5)).await;
    assert_eq!((o.counts.updated, o.counts.deleted), (1, 1));
    assert_eq!(
        cal.event("101", "mensetsuyoteibi")["start"]["dateTime"],
        "2026-11-06T14:00:00+09:00"
    );
    assert_eq!(cal.event("101", "naiteinichi")["status"], "cancelled");
    assert!(cal
        .calls()
        .contains(&format!("delete {}", event_id("101", "naiteinichi"))));
}

#[tokio::test]
async fn type_change_recreates_events_on_next_cycle() {
    let (hs, cal) = (hubspot(), FakeCal::default());
    let mut c = Cycle::default();
    run(&mut c, &hs, &cal, now()).await;
    // HubSpot で面接予定日を日付だけの型に変えた (応募自体は更新されていない)。定義は読み直す時期とする
    *hs.defs.lock().unwrap() = defs("date");
    c.forget_definitions();
    let o = run(&mut c, &hs, &cal, now() + chrono::Duration::minutes(5)).await;
    assert_eq!((o.counts.updated, o.counts.errors), (1, 0));
    let e = cal.event("101", "mensetsuyoteibi");
    assert_eq!(e["start"], json!({"date": "2026-11-05"}));
    assert_eq!(e["end"], json!({"date": "2026-11-06"}));
    // 型が変わった項目は値の範囲でも読む
    let resync = hs.searches.lock().unwrap()[2].clone();
    assert_eq!(
        resync["filterGroups"][0]["filters"][0]["propertyName"],
        "mensetsuyoteibi"
    );
    // 戻すと時刻つきに戻る
    *hs.defs.lock().unwrap() = defs("datetime");
    c.forget_definitions();
    run(&mut c, &hs, &cal, now() + chrono::Duration::minutes(10)).await;
    let e = cal.event("101", "mensetsuyoteibi");
    assert_eq!(
        e["start"],
        json!({"dateTime": "2026-11-05T10:30:00+09:00", "timeZone": "Asia/Tokyo"})
    );
}

#[tokio::test]
async fn existing_id_conflict_is_overwritten_and_confirmed() {
    let (hs, cal) = (hubspot(), FakeCal::default());
    // 前に消した予定が同じ ID で残っている (Google は 409 を返す)
    let id = event_id("101", "mensetsuyoteibi");
    cal.events.lock().unwrap().insert(
        id.clone(),
        json!({"id": id, "status": "cancelled", "summary": "古い", "start": {"date": "2026-01-01"}, "end": {"date": "2026-01-02"}}),
    );
    let mut c = Cycle::default();
    let o = run(&mut c, &hs, &cal, now()).await;
    assert_eq!((o.counts.created, o.counts.updated), (1, 1));
    let e = cal.event("101", "mensetsuyoteibi");
    assert_eq!(e["status"], "confirmed");
    assert_eq!(
        e["start"],
        json!({"dateTime": "2026-11-05T10:30:00+09:00", "timeZone": "Asia/Tokyo"})
    );
    let calls = cal.calls();
    let i = calls
        .iter()
        .position(|c| *c == format!("insert {id}"))
        .unwrap();
    assert_eq!(calls[i + 1], format!("patch {id}"));
}

/// 報告シートの URL を持つ取引を無くし、取引の一覧を差し替える
fn deals_without_sheet(hs: &FakeHs, deals: Vec<HubSpotRecord>) {
    let ids: Vec<String> = deals.iter().map(|d| d.id.clone()).collect();
    hs.records.lock().unwrap().insert("deals".into(), deals);
    hs.assoc.lock().unwrap().insert(
        ("0-420".into(), "deals".into()),
        BTreeMap::from([("501".into(), ids)]),
    );
}

async fn guest_of(hs: &FakeHs) -> Value {
    let cal = FakeCal::default();
    let mut c = Cycle::default();
    run(&mut c, hs, &cal, now()).await;
    cal.event("101", "mensetsuyoteibi")["attendees"].clone()
}

#[tokio::test]
async fn guest_falls_back_to_ranked_deal_without_sheet_url() {
    let hs = hubspot();
    // どれも報告シートの URL なし。終わった契約 (O3、開始日が最新) < 生きているオプション (O4) < 生きている主契約 (O2)
    deals_without_sheet(
        &hs,
        vec![
            rec(
                "903",
                &[
                    ("dealstage", "999"),
                    ("dealname", "終わった契約"),
                    ("contract_start_date", "2026-09-01"),
                    ("hubspot_owner_id", "O3"),
                ],
            ),
            rec(
                "904",
                &[
                    ("dealstage", "52016153"),
                    ("dealname", "求人追加 秋"),
                    ("contract_start_date", "2026-08-01"),
                    ("hubspot_owner_id", "O4"),
                ],
            ),
            rec(
                "902",
                &[
                    ("dealstage", "52016153"),
                    ("dealname", "主契約"),
                    ("contract_start_date", "2026-04-01"),
                    ("hubspot_owner_id", "O2"),
                ],
            ),
        ],
    );
    // 報告シートの選び方は変えない (URL が無ければ選ばない)
    let deals = hs.records.lock().unwrap()["deals"].clone();
    let ids: Vec<String> = vec!["903".into(), "904".into(), "902".into()];
    assert!(crate::crm::applicants::report_deal(&ids, &deals).is_none());
    assert_eq!(
        crate::crm::applicants::ranked_deal(&ids, &deals)
            .unwrap()
            .id,
        "902"
    );
    assert_eq!(
        guest_of(&hs).await,
        json!([{ "email": "other@example.com" }])
    );

    // 生きている主契約が無ければ生きているオプション
    deals_without_sheet(
        &hs,
        vec![
            rec(
                "903",
                &[
                    ("dealstage", "999"),
                    ("contract_start_date", "2026-09-01"),
                    ("hubspot_owner_id", "O3"),
                ],
            ),
            rec(
                "904",
                &[
                    ("dealstage", "1049738304"),
                    ("contract_start_date", "2026-01-01"),
                    ("hubspot_owner_id", "O4"),
                ],
            ),
        ],
    );
    assert_eq!(
        guest_of(&hs).await,
        json!([{ "email": "option@example.com" }])
    );

    // どれも生きていなければ全体から、契約開始日が新しい方
    deals_without_sheet(
        &hs,
        vec![
            rec(
                "905",
                &[
                    ("dealstage", "999"),
                    ("contract_start_date", "2025-01-01"),
                    ("hubspot_owner_id", "O2"),
                ],
            ),
            rec(
                "903",
                &[
                    ("dealstage", "998"),
                    ("contract_start_date", "2025-06-01"),
                    ("hubspot_owner_id", "O3"),
                ],
            ),
        ],
    );
    assert_eq!(
        guest_of(&hs).await,
        json!([{ "email": "ended@example.com" }])
    );
}

#[tokio::test]
async fn ranked_deal_ties_break_by_createdate_then_association_order() {
    let hs = hubspot();
    // 契約開始日が同じなら作成日が新しい方
    deals_without_sheet(
        &hs,
        vec![
            rec(
                "902",
                &[
                    ("dealstage", "52016153"),
                    ("contract_start_date", "2026-04-01"),
                    ("createdate", "2026-03-01T00:00:00Z"),
                    ("hubspot_owner_id", "O2"),
                ],
            ),
            rec(
                "906",
                &[
                    ("dealstage", "52016153"),
                    ("contract_start_date", "2026-04-01"),
                    ("createdate", "2026-03-05T00:00:00Z"),
                    ("hubspot_owner_id", "O4"),
                ],
            ),
        ],
    );
    assert_eq!(
        guest_of(&hs).await,
        json!([{ "email": "option@example.com" }])
    );
    // 何もかも同じなら関連の先勝ち (ここでは 906 が先)
    deals_without_sheet(
        &hs,
        vec![
            rec(
                "906",
                &[
                    ("dealstage", "52016153"),
                    ("contract_start_date", "2026-04-01"),
                    ("hubspot_owner_id", "O4"),
                ],
            ),
            rec(
                "902",
                &[
                    ("dealstage", "52016153"),
                    ("contract_start_date", "2026-04-01"),
                    ("hubspot_owner_id", "O2"),
                ],
            ),
        ],
    );
    assert_eq!(
        guest_of(&hs).await,
        json!([{ "email": "option@example.com" }])
    );
    // URL を持つ取引があれば、順位が上の URL なしの取引より URL のある取引 (今の選び方のまま)
    deals_without_sheet(
        &hs,
        vec![
            rec(
                "902",
                &[
                    ("dealstage", "52016153"),
                    ("contract_start_date", "2026-09-01"),
                    ("hubspot_owner_id", "O2"),
                ],
            ),
            rec(
                "907",
                &[
                    ("dealstage", "999"),
                    (
                        "customer_sheet_url",
                        "https://docs.google.com/spreadsheets/d/y",
                    ),
                    ("hubspot_owner_id", "O3"),
                ],
            ),
        ],
    );
    assert_eq!(
        guest_of(&hs).await,
        json!([{ "email": "ended@example.com" }])
    );
}

#[tokio::test]
async fn no_guest_when_no_deal_is_readable_and_company_falls_back() {
    let (hs, cal) = (hubspot(), FakeCal::default());
    // 関連する取引が読めない → 担当を決めない (ゲストなし)。会社名も取れず、応募先取引名を使う
    hs.records.lock().unwrap().insert("deals".into(), vec![]);
    let mut c = Cycle::default();
    run(&mut c, &hs, &cal, now()).await;
    let e = cal.event("101", "mensetsuyoteibi");
    assert_eq!(e["attendees"], json!([]));
    assert_eq!(e["summary"], "面接｜山田 太郎｜取引名の会社");
}

#[tokio::test]
async fn events_of_vanished_applicants_are_deleted() {
    let (hs, cal) = (hubspot(), FakeCal::default());
    let mut c = Cycle::default();
    run(&mut c, &hs, &cal, now()).await;
    // 応募 101 が HubSpot から消えた (Search にも batch read にも出ない)
    hs.applicants.lock().unwrap().clear();
    hs.records.lock().unwrap().insert("0-421".into(), vec![]);
    let mut c2 = Cycle::default();
    let o = c2
        .run_once(
            &hs,
            &cal,
            &cfg(),
            &Params {
                pacing: Duration::ZERO,
                reap_every: Duration::ZERO,
            },
            now(),
            PORTAL,
        )
        .await;
    assert_eq!(o.counts.deleted, 2);
    assert_eq!(cal.event("101", "mensetsuyoteibi")["status"], "cancelled");
    assert_eq!(cal.event("101", "naiteinichi")["status"], "cancelled");
}

#[tokio::test]
async fn living_applicants_are_kept_by_reaping() {
    let (hs, cal) = (hubspot(), FakeCal::default());
    let mut c = Cycle::default();
    run(&mut c, &hs, &cal, now()).await;
    // 応募は残っているが、今回の Search には出ない (更新されていない)
    let mut c2 = Cycle::default();
    hs.applicants.lock().unwrap().clear();
    let o = c2
        .run_once(
            &hs,
            &cal,
            &cfg(),
            &Params {
                pacing: Duration::ZERO,
                reap_every: Duration::ZERO,
            },
            now(),
            PORTAL,
        )
        .await;
    assert_eq!(o.counts.deleted, 0);
    assert_eq!(cal.event("101", "mensetsuyoteibi")["status"], "confirmed");
}

#[test]
fn date_search_splits_over_five_props() {
    let props: Vec<String> = (0..7).map(|i| format!("p{i}")).collect();
    let bodies = date_search_bodies(&props, 1, &[]);
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0]["filterGroups"].as_array().unwrap().len(), 5);
    assert_eq!(bodies[1]["filterGroups"].as_array().unwrap().len(), 2);
}
