//! 社内の管理 API (顧客・シート・項目・試しに読む) の HTTP のテスト。偽の Sheets で動かす。
use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::Router;
use chrono::Utc;
use serde_json::{json, Value};

use super::super::sheets::GoogleError;
use super::super::store::CustomerStage;
use super::super::test_fake::FakeSheets;
use super::super::{router_parts, Parts};
use super::{call, cfg, inject_staff, pair, store_c1, test_state, FakeMailer, PW};
use crate::crm::rbac::{CrmAccess, CrmRole};

const BOSS: &str = "boss@f-a-c.co.jp";
const STAFF: &str = "staff@f-a-c.co.jp";
const SHEET_URL: &str =
    "https://docs.google.com/spreadsheets/d/1TestSheetIdForPortal_abcdefghij/edit#gid=777";
/// 架空の応募者 (テストの値。ログに出ないことを確かめる)
const APPLICANT: &str = "架空 応募者一郎";
const PHONE: &str = "090-0000-9999";

fn reader() -> Arc<FakeSheets> {
    Arc::new(
        FakeSheets::default()
            .with_tab(
                "応募者情報管理",
                777,
                vec![
                    vec!["応募日", "応募者名", "電話番号", "状況", "備考"],
                    vec!["記入例", "記入例 太郎", "090-0000-0000", "未連絡", ""],
                    vec![
                        "2026/10/20",
                        APPLICANT,
                        PHONE,
                        "面接調整中",
                        "=IMPORTRANGE(\"x\",\"y\")",
                    ],
                    vec!["2026/10/21", "架空 次郎", "090-0000-0002", "未連絡", ""],
                ],
            )
            .with_tab("集計", 5, vec![vec!["合計"]]),
    )
}

struct H {
    app: Router,
    auth: Arc<super::super::auth::PortalAuth>,
    reader: Arc<FakeSheets>,
    boss: Vec<String>,
    staff: Vec<String>,
}

async fn harness_with(reader: Arc<FakeSheets>) -> H {
    harness_store(reader, None).await
}

async fn harness_store(reader: Arc<FakeSheets>, store_spreadsheet: Option<&str>) -> H {
    let auth = Arc::new(super::super::auth::PortalAuth::new(store_c1(), cfg()));
    let parts = Parts {
        auth: Some(auth.clone()),
        mailer: Some(Arc::new(FakeMailer::default())),
        reader: Some(reader.clone()),
        store_spreadsheet: store_spreadsheet.map(str::to_string),
    };
    let access = CrmAccess::from_list(&format!("{BOSS},{STAFF}"))
        .with_test_role(BOSS, CrmRole::Admin)
        .with_test_role(STAFF, CrmRole::Bpo);
    let app = Router::new()
        .merge(router_parts(parts, access, None))
        .route("/__test/session", axum::routing::post(inject_staff))
        .with_state(test_state())
        .layer(tower_sessions::SessionManagerLayer::new(
            tower_sessions::MemoryStore::default(),
        ));
    let mut cookies = Vec::new();
    for who in [BOSS, STAFF] {
        let (_, set, _) = call(
            &app,
            "POST",
            "/__test/session",
            &[],
            Some(json!({ "email": who })),
            false,
        )
        .await;
        cookies.push(vec![pair(&set, "id").expect("社内のセッション")]);
    }
    let staff = cookies.pop().unwrap();
    let boss = cookies.pop().unwrap();
    H {
        app,
        auth,
        reader,
        boss,
        staff,
    }
}

async fn boss(h: &H, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let (s, _, v) = call(&h.app, method, uri, &h.boss, body, true).await;
    (s, v)
}

/// 計画 10 章 PR1 の完了の条件: 試すシートを登録し、試しに読めることを確かめる
#[tokio::test]
async fn 顧客とシートを登録し_試しに読み_項目を設定して招待を出せる() {
    let h = harness_with(reader()).await;

    let (s, v) = boss(
        &h,
        "POST",
        "/api/admin/portal/customers",
        Some(json!({ "name": " 架空の運送株式会社 ", "hubspot_company_id": "123456" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "架空の運送株式会社");
    assert_eq!(v["stage"], "sheet_only", "最初は「シートだけ」");
    let cid = v["id"].as_str().unwrap().to_string();

    let (s, v) = boss(
        &h,
        "POST",
        "/api/admin/portal/sheet-probe",
        Some(json!({ "url": SHEET_URL })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["gid"], 777);
    assert!(v["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["sheet_id"] == 777 && t["title"] == "応募者情報管理"));

    let (s, v) = boss(
        &h,
        "POST",
        "/api/admin/portal/sheet-preview",
        Some(json!({ "url": SHEET_URL, "sheet_id": 777, "header_row": 1, "example_rows": 1 })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        v["headers"],
        json!(["応募日", "応募者名", "電話番号", "状況", "備考"])
    );
    assert_eq!(
        v["examples"][0][1], "記入例 太郎",
        "記入例の行は応募者と分ける"
    );
    assert_eq!(v["rows"].as_array().unwrap().len(), 2);
    assert_eq!(v["rows"][0][1], APPLICANT);
    assert_eq!(
        v["rows"][0][4], "=IMPORTRANGE(\"x\",\"y\")",
        "数式に見える値も文字のまま返す"
    );

    let add = json!({ "url": SHEET_URL, "sheet_id": 777, "label": "応募者", "header_row": 1, "example_rows": 1 });
    let (s, v) = boss(
        &h,
        "POST",
        &format!("/api/admin/portal/customers/{cid}/sheets"),
        Some(add.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["tab_title"], "応募者情報管理");
    assert_eq!(v["visible_columns"], 0, "最初は何も見せない");
    let sid = v["id"].as_str().unwrap().to_string();

    // 同じシートの同じタブは、別の顧客にも二重に登録させない
    let (s, _) = boss(
        &h,
        "POST",
        "/api/admin/portal/customers/c1/sheets",
        Some(add),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (s, v) = boss(
        &h,
        "GET",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let cols = v["columns"].as_array().unwrap();
    assert_eq!(cols.len(), 5);
    assert!(cols
        .iter()
        .all(|c| c["visible"] == false && c["writable"] == false));

    // 見せる項目が無いうちは「両方」に進めない
    let (s, v) = boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/customers/{cid}"),
        Some(json!({ "stage": "both" })),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");

    let col = |header: &str, visible: bool, writable: bool| json!({ "header": header, "label": "", "visible": visible, "writable": writable, "in_list": visible, "multiline": false, "kind": "text", "group": "summary" });
    let (s, _) = boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        Some(json!({ "columns": [col("状況", false, true)] })),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "見せない項目は書けるようにできない"
    );
    let (s, _) = boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        Some(json!({ "columns": [col("状況", true, true), col("状況", true, false)] })),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "同じ見出しが 2 つ");
    let (s, v) = boss(&h, "PUT", &format!("/api/admin/portal/sheets/{sid}/columns"), Some(json!({ "columns": [col("応募者名", true, false), col("状況", true, true), col("電話番号", false, false)] }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["sheet"]["visible_columns"], 2);
    assert_eq!(v["columns"][1]["writable"], true);

    let (s, v) = boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/customers/{cid}"),
        Some(json!({ "stage": "both" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["stage"], "both");

    let (s, v) = boss(
        &h,
        "POST",
        &format!("/api/admin/portal/sheets/{sid}/preview"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["rows"][1][1], "架空 次郎");

    let (s, _) = boss(
        &h,
        "POST",
        "/api/admin/portal/accounts",
        Some(json!({ "email": "tanto@customer.example", "customer_id": "c-nope" })),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "登録の無い顧客にはアカウントを作らない"
    );
    let (s, v) = boss(
        &h,
        "POST",
        "/api/admin/portal/accounts",
        Some(json!({ "email": "tanto@customer.example", "customer_id": cid })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["invite_path"]
        .as_str()
        .unwrap()
        .starts_with("/portal/invite#t="));

    let (s, v) = boss(
        &h,
        "GET",
        &format!("/api/admin/portal/customers/{cid}"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["sheets"].as_array().unwrap().len(), 1);
    assert_eq!(v["accounts"][0]["email"], "tanto@customer.example");
    assert_eq!(v["customer"]["account_count"], 1);

    let (s, v) = boss(&h, "GET", "/api/admin/portal/activity", None).await;
    assert_eq!(s, StatusCode::OK);
    let kinds: Vec<&str> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    for k in [
        "customer_add",
        "sheet_preview",
        "sheet_add",
        "column_change",
        "customer_change",
        "account_invite",
    ] {
        assert!(kinds.contains(&k), "記録に {k} が無い: {kinds:?}");
    }
    assert!(v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["actor"] == BOSS));
    assert!(
        !v.to_string().contains(APPLICANT),
        "記録に応募者の値を入れない"
    );

    let (s, v) = boss(&h, "GET", "/api/admin/portal/status", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["store_kind"], "memory");
    assert_eq!(v["ready"], true);
    assert_eq!(v["customer_login"], true);
    assert_eq!(
        v["reader_email"],
        "reader@test-project.iam.gserviceaccount.com"
    );
}

#[tokio::test]
async fn 管理者でない社内の人は403_書き込みはcsrfの確認が要る() {
    let h = harness_with(reader()).await;
    let routes: &[(&str, &str)] = &[
        ("GET", "/api/admin/portal/status"),
        ("GET", "/api/admin/portal/accounts"),
        ("POST", "/api/admin/portal/accounts"),
        ("GET", "/api/admin/portal/customers"),
        ("POST", "/api/admin/portal/customers"),
        ("GET", "/api/admin/portal/customers/c1"),
        ("PUT", "/api/admin/portal/customers/c1"),
        ("POST", "/api/admin/portal/customers/c1/sheets"),
        ("POST", "/api/admin/portal/sheet-probe"),
        ("POST", "/api/admin/portal/sheet-preview"),
        ("PUT", "/api/admin/portal/sheets/s-x"),
        ("POST", "/api/admin/portal/sheets/s-x/preview"),
        ("GET", "/api/admin/portal/sheets/s-x/columns"),
        ("PUT", "/api/admin/portal/sheets/s-x/columns"),
        ("GET", "/api/admin/portal/activity"),
    ];
    let body = json!({ "name": "x", "url": SHEET_URL, "sheet_id": 777, "label": "x", "header_row": 1, "email": "x@example.com", "customer_id": "c1", "columns": [] });
    for (m, uri) in routes {
        let (s, _, _) = call(&h.app, m, uri, &h.staff, Some(body.clone()), true).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "管理者でない人: {m} {uri}");
        let (s, _, _) = call(&h.app, m, uri, &[], Some(body.clone()), true).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "未ログイン: {m} {uri}");
        if *m != "GET" {
            let (s, _, v) = call(&h.app, m, uri, &h.boss, Some(body.clone()), false).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::FORBIDDEN, Some("csrf")),
                "X-Requested-With が無い: {m} {uri}"
            );
        }
    }
    // 別のオリジンからの書き込みは管理者でも断る
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/admin/portal/customers")
        .header("x-requested-with", "fetch")
        .header("origin", "https://evil.example")
        .header(axum::http::header::COOKIE, h.boss.join("; "))
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(json!({ "name": "x" }).to_string()))
        .unwrap();
    let res = tower::ServiceExt::oneshot(h.app.clone(), req)
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert!(
        h.auth.store().customers().iter().all(|c| c.name != "x"),
        "何も作らない"
    );
}

#[tokio::test]
async fn 共有されていないシートは追加するアドレスを返し_urlが違えば読みにいかない() {
    let r = reader();
    *r.fail_reads.lock().unwrap() = Some(GoogleError::Status(403));
    let h = harness_with(r).await;
    let (s, v) = boss(
        &h,
        "POST",
        "/api/admin/portal/sheet-probe",
        Some(json!({ "url": SHEET_URL })),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(v["error"], "sheet_not_shared");
    assert!(v["message"]
        .as_str()
        .unwrap()
        .contains("reader@test-project.iam.gserviceaccount.com"));
    let (s, _) = boss(
        &h,
        "POST",
        "/api/admin/portal/customers/c1/sheets",
        Some(json!({ "url": SHEET_URL, "sheet_id": 777, "label": "応募者", "header_row": 1 })),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "開けないシートは登録しない");
    assert!(h.auth.store().sheets(None).is_empty());

    h.reader.calls.lock().unwrap().clear();
    for bad in [
        "https://evil.example/spreadsheets/d/1TestSheetIdForPortal_abcdefghij",
        "javascript:alert(1)",
        "",
    ] {
        let (s, _) = boss(
            &h,
            "POST",
            "/api/admin/portal/sheet-probe",
            Some(json!({ "url": bad })),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    assert!(
        h.reader.calls.lock().unwrap().is_empty(),
        "Google を呼ばない"
    );

    // 登録の無いタブ
    *h.reader.fail_reads.lock().unwrap() = None;
    let (s, v) = boss(
        &h,
        "POST",
        "/api/admin/portal/sheet-preview",
        Some(json!({ "url": SHEET_URL, "sheet_id": 999, "header_row": 1 })),
    )
    .await;
    assert_eq!(
        (s, v["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("tab_not_found"))
    );
}

/// 顧客を止める・「シートだけ」に戻すと、その顧客のアカウントはすぐ入れない
#[tokio::test]
async fn 顧客を止めると_その顧客の全員がすぐ入れない() {
    let h = harness_with(reader()).await;
    let auth = &h.auth;
    let (_, token) = auth
        .create_account("a@example.com", "c1", Utc::now())
        .unwrap();
    auth.accept_invite(&token, PW, Utc::now()).unwrap();
    let super::super::auth::LoginResult::CodeRequired {
        challenge, code, ..
    } = auth.login("a@example.com", PW, None, "ip", Utc::now())
    else {
        panic!()
    };
    let super::super::auth::CodeResult::SignedIn { session, .. } =
        auth.verify_code(&challenge, &code, true, Utc::now())
    else {
        panic!()
    };
    assert!(auth.current(&session, Utc::now()).is_some());

    let (s, _) = boss(
        &h,
        "PUT",
        "/api/admin/portal/customers/c1",
        Some(json!({ "disabled": true })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        auth.current(&session, Utc::now()).is_none(),
        "止めた顧客のログイン中の画面"
    );
    assert!(matches!(
        auth.login("a@example.com", PW, None, "ip2", Utc::now()),
        super::super::auth::LoginResult::Invalid
    ));

    let mut c = auth.store().customer("c1").unwrap();
    c.disabled = false;
    c.stage = CustomerStage::SheetOnly;
    auth.store().update_customer(c).unwrap();
    assert!(
        auth.current(&session, Utc::now()).is_none(),
        "「シートだけ」の顧客は画面に入れない"
    );
}

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

/// 管理の操作と顧客のログインで、応募者の値・顧客名・メールアドレス・パスワード・招待の値をログに出さない
#[tokio::test]
async fn ログに個人情報と鍵を出さない() {
    let logs = LogBuf::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let h = harness_with(reader()).await;
    let (_, v) = boss(
        &h,
        "POST",
        "/api/admin/portal/customers",
        Some(json!({ "name": "架空の秘密顧客" })),
    )
    .await;
    let cid = v["id"].as_str().unwrap().to_string();
    let (_, v) = boss(&h, "POST", &format!("/api/admin/portal/customers/{cid}/sheets"), Some(json!({ "url": SHEET_URL, "sheet_id": 777, "label": "応募者", "header_row": 1, "example_rows": 1 }))).await;
    let sid = v["id"].as_str().unwrap().to_string();
    boss(
        &h,
        "POST",
        &format!("/api/admin/portal/sheets/{sid}/preview"),
        None,
    )
    .await;
    let (_, v) = boss(
        &h,
        "POST",
        "/api/admin/portal/accounts",
        Some(json!({ "email": "secret.person@customer.example", "customer_id": "c1" })),
    )
    .await;
    let token = v["invite_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/portal/invite#t=")
        .unwrap()
        .to_string();
    let (s, _, _) = call(
        &h.app,
        "POST",
        "/api/portal/invite/accept",
        &[],
        Some(json!({ "token": token, "password": PW })),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = call(
        &h.app,
        "POST",
        "/api/portal/login",
        &[],
        Some(json!({ "email": "secret.person@customer.example", "password": PW })),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    call(
        &h.app,
        "POST",
        "/api/portal/login",
        &[],
        Some(json!({ "email": "secret.person@customer.example", "password": "wrong password!!" })),
        true,
    )
    .await;

    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("[portal]"), "ログを拾えている: {text}");
    for secret in [
        APPLICANT,
        PHONE,
        "架空の秘密顧客",
        "secret.person",
        PW,
        token.as_str(),
        "wrong password",
    ] {
        assert!(!text.contains(secret), "ログに「{secret}」が出た:\n{text}");
    }
}

// ---- レビューの指摘への対応 (2026-10-26) ----

async fn add_customer_and_sheet(h: &H) -> (String, String) {
    let (_, v) = boss(
        h,
        "POST",
        "/api/admin/portal/customers",
        Some(json!({ "name": "架空の顧客" })),
    )
    .await;
    let cid = v["id"].as_str().unwrap().to_string();
    let (s, v) = boss(
        h,
        "POST",
        &format!("/api/admin/portal/customers/{cid}/sheets"),
        Some(json!({ "url": SHEET_URL, "sheet_id": 777, "label": "応募者", "header_row": 1, "example_rows": 1 })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    (cid, v["id"].as_str().unwrap().to_string())
}

/// 見出しのセル内の改行は空白 1 つにそろえ、そのまま項目の設定を保存できる
#[tokio::test]
async fn 見出しの改行は空白にそろえて項目の設定を保存できる() {
    let r = Arc::new(FakeSheets::default().with_tab(
        "応募者",
        777,
        vec![
            vec!["応募\n日", "面接\r\n予定日", "状況"],
            vec!["2026/10/20", "", "未連絡"],
        ],
    ));
    let h = harness_with(r).await;
    let (_, sid) = add_customer_and_sheet(&h).await;
    let (_, v) = boss(
        &h,
        "GET",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        None,
    )
    .await;
    let headers: Vec<&str> = v["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["header"].as_str().unwrap())
        .collect();
    assert_eq!(headers, vec!["応募 日", "面接 予定日", "状況"]);
    let col = |h: &str| json!({ "header": h, "label": "", "visible": true, "writable": false, "in_list": true, "multiline": false, "kind": "text", "group": "summary" });
    let (s, v) = boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        Some(json!({ "columns": [col("応募 日"), col("面接\n予定日")] })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        v["columns"][1]["header"], "面接 予定日",
        "保存のときも同じにそろえる"
    );
}

/// 社内用シートは、試しに読む・顧客のシートとして登録することを拒む
#[tokio::test]
async fn 社内用シートは試しに読めず登録もできない() {
    let h = harness_store(reader(), Some("1TestSheetIdForPortal_abcdefghij")).await;
    for (path, body) in [
        ("/api/admin/portal/sheet-probe", json!({ "url": SHEET_URL })),
        (
            "/api/admin/portal/sheet-preview",
            json!({ "url": SHEET_URL, "sheet_id": 777, "header_row": 1 }),
        ),
        (
            "/api/admin/portal/customers/c1/sheets",
            json!({ "url": SHEET_URL, "sheet_id": 777, "label": "x", "header_row": 1 }),
        ),
    ] {
        let (s, v) = boss(&h, "POST", path, Some(body)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert!(v["message"].as_str().unwrap().contains("社内用"));
    }
    assert!(
        h.reader.calls.lock().unwrap().is_empty(),
        "Google を呼ばない"
    );
    assert!(h.auth.store().sheets(None).is_empty());
}

/// 試しに読んだ記録に、スプレッドシートとタブを残す
#[tokio::test]
async fn 試しに読んだ記録にスプレッドシートとタブを残す() {
    let h = harness_with(reader()).await;
    let (_, sid) = add_customer_and_sheet(&h).await;
    let (s, _) = boss(
        &h,
        "POST",
        &format!("/api/admin/portal/sheets/{sid}/preview"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let logs = h.auth.store().recent_logs(10);
    let previews: Vec<_> = logs.iter().filter(|e| e.kind == "sheet_preview").collect();
    assert_eq!(
        previews.len(),
        1,
        "登録の前の読み取りは別の記録。登録したシートを読んだのは 1 回"
    );
    assert!(previews[0]
        .detail
        .contains("1TestSheetIdForPortal_abcdefghij"));
    assert!(previews[0].detail.contains("777"));
    assert!(previews[0].detail.contains("応募者情報管理"));
    assert_eq!(previews[0].sheet_reg_id, sid);
}

/// 試しに読むは POST (副作用があるので CSRF を通す)。GET では読めない
#[tokio::test]
async fn 試しに読むはpostでcsrfが要る() {
    let h = harness_with(reader()).await;
    let (_, sid) = add_customer_and_sheet(&h).await;
    let n = h.auth.store().recent_logs(50).len();
    let (s, _, _) = call(
        &h.app,
        "GET",
        &format!("/api/admin/portal/sheets/{sid}/preview"),
        &h.boss,
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
    let (s, _, _) = call(
        &h.app,
        "POST",
        &format!("/api/admin/portal/sheets/{sid}/preview"),
        &h.boss,
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(
        h.auth.store().recent_logs(50).len(),
        n,
        "断った読み取りは記録しない"
    );
}

/// 見出しの行を変えたら、その行の見出しで項目を作り直す (同じ見出しの設定は引き継ぐ)
#[tokio::test]
async fn 見出しの行を変えたら項目を作り直す() {
    let r = Arc::new(FakeSheets::default().with_tab(
        "応募者",
        777,
        vec![
            vec!["注記"],
            vec!["応募日", "状況", "備考"],
            vec!["2026/10/20", "未連絡", ""],
        ],
    ));
    let h = harness_with(r).await;
    let (_, sid) = add_customer_and_sheet(&h).await;
    let (_, v) = boss(
        &h,
        "GET",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        None,
    )
    .await;
    assert_eq!(
        v["columns"].as_array().unwrap().len(),
        1,
        "1 行目は注記だけ"
    );
    let (s, v) = boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/sheets/{sid}"),
        Some(json!({ "header_row": 2, "example_rows": 0 })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = boss(
        &h,
        "GET",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        None,
    )
    .await;
    let headers: Vec<&str> = v["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["header"].as_str().unwrap())
        .collect();
    assert_eq!(headers, vec!["応募日", "状況", "備考"]);
    // 設定を入れてから同じ見出しの行に戻すと、設定を引き継ぐ
    let col = |h: &str, vis: bool| json!({ "header": h, "label": "", "visible": vis, "writable": false, "in_list": vis, "multiline": false, "kind": "text", "group": "summary" });
    boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        Some(json!({ "columns": [col("応募日", false), col("状況", true), col("備考", false)] })),
    )
    .await;
    boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/sheets/{sid}"),
        Some(json!({ "header_row": 1 })),
    )
    .await;
    boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/sheets/{sid}"),
        Some(json!({ "header_row": 2 })),
    )
    .await;
    let (_, v) = boss(
        &h,
        "GET",
        &format!("/api/admin/portal/sheets/{sid}/columns"),
        None,
    )
    .await;
    let cols = v["columns"].as_array().unwrap();
    assert_eq!(cols.len(), 3);
    assert_eq!(
        cols[1]["visible"], false,
        "1 行目に戻したときに消えた見出しの設定は引き継がない"
    );
    // 読めなければ何も変えない
    *h.reader.fail_reads.lock().unwrap() = Some(GoogleError::Status(500));
    let (s, _) = boss(
        &h,
        "PUT",
        &format!("/api/admin/portal/sheets/{sid}"),
        Some(json!({ "header_row": 1 })),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert_eq!(h.auth.store().sheet(&sid).unwrap().header_row, 2);
}

/// 顧客を止めると、その顧客のアカウントのログイン中の画面・覚えた端末・招待を消し、再開しても生き返らない
#[tokio::test]
async fn 顧客を止めると端末と画面と招待を消し再開しても生き返らない() {
    let h = harness_with(reader()).await;
    let auth = &h.auth;
    let (a, token) = auth
        .create_account("a@example.com", "c1", Utc::now())
        .unwrap();
    auth.accept_invite(&token, PW, Utc::now()).unwrap();
    let super::super::auth::LoginResult::CodeRequired {
        challenge, code, ..
    } = auth.login("a@example.com", PW, None, "ip", Utc::now())
    else {
        panic!()
    };
    let super::super::auth::CodeResult::SignedIn { session, device } =
        auth.verify_code(&challenge, &code, true, Utc::now())
    else {
        panic!()
    };
    let (_, b_token) = auth
        .create_account("b@example.com", "c1", Utc::now())
        .unwrap();

    let (s, _) = boss(
        &h,
        "PUT",
        "/api/admin/portal/customers/c1",
        Some(json!({ "disabled": true })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = boss(
        &h,
        "PUT",
        "/api/admin/portal/customers/c1",
        Some(json!({ "disabled": false })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        auth.current(&session, Utc::now()).is_none(),
        "ログイン中の画面は戻らない"
    );
    assert!(
        matches!(
            auth.login("a@example.com", PW, device.as_deref(), "ip", Utc::now()),
            super::super::auth::LoginResult::CodeRequired { .. }
        ),
        "覚えた端末は戻らない (パスワードとコードで入り直す)"
    );
    assert!(
        auth.invite_info(&b_token, Utc::now()).is_err(),
        "招待は戻らない"
    );
    assert_eq!(
        auth.store().account(&a.id).unwrap().status,
        super::super::store::AccountStatus::Active
    );
    assert!(h
        .auth
        .store()
        .recent_logs(20)
        .iter()
        .any(|e| e.kind == "customer_revoke"));
}

#[test]
fn 二つの鍵が同じサービスアカウントなら顧客のシート側を使わない() {
    let a: Arc<dyn super::super::sheets::SheetsApi> = Arc::new(FakeSheets::default());
    let b = FakeSheets::default();
    assert!(super::super::distinct_reader(Some(a.clone()), Some(&b)).is_none());
    assert!(super::super::distinct_reader(Some(a.clone()), None).is_some());
    struct Other;
    impl super::super::sheets::SheetsApi for Other {
        fn client_email(&self) -> &str {
            "store@other.iam.gserviceaccount.com"
        }
        fn tabs<'a>(
            &'a self,
            _: &'a str,
        ) -> super::super::sheets::ApiFuture<'a, Vec<super::super::sheets::TabInfo>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn add_tabs<'a>(
            &'a self,
            _: &'a str,
            _: &'a [String],
        ) -> super::super::sheets::ApiFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
        fn batch_get<'a>(
            &'a self,
            _: &'a str,
            _: &'a [String],
        ) -> super::super::sheets::ApiFuture<'a, Vec<Vec<Vec<String>>>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn batch_update<'a>(
            &'a self,
            _: &'a str,
            _: &'a [(String, Vec<Vec<String>>)],
        ) -> super::super::sheets::ApiFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
        fn append<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: &'a [Vec<String>],
        ) -> super::super::sheets::ApiFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }
    assert!(super::super::distinct_reader(Some(a), Some(&Other)).is_some());
}
