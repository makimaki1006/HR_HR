//! 顧客の見る画面の API (`/api/portal/sheets*`) のテスト。偽の Sheets / Drive で動かす。
//! 値はすべて架空。
use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, Request, StatusCode};
use axum::Router;
use chrono::Utc;
use serde_json::Value;

use super::super::sheets::GoogleError;
use super::super::store::{
    ColumnGroup, ColumnKind, ColumnSetting, Customer, CustomerStage, PortalStore, SheetReg,
};
use super::super::test_fake::FakeSheets;
use super::super::{router_parts, Parts, SESSION_COOKIE};
use super::{call, cfg, store_c1, test_state, FakeMailer, PW};
use crate::crm::rbac::CrmAccess;

const SECRET_MEMO: &str = "社内だけの架空メモ";
const SECRET_HEADER: &str = "社内メモ";
const APPLICANT: &str = "架空 花子";
const OTHER_APPLICANT: &str = "別顧客 太郎";
const STORE_SHEET: &str = "STORE-spreadsheet-0000000000";

fn fake() -> Arc<FakeSheets> {
    let f = FakeSheets::default()
        .with_tab(
            "応募者情報管理",
            777,
            vec![
                vec!["応募日", "応募者名", "電話番号", "状況", SECRET_HEADER],
                vec!["記入例", "記入例 太郎", "090-0000-0000", "未連絡", ""],
                vec![
                    "2026/10/20",
                    APPLICANT,
                    "090-0000-0001",
                    "面接調整中",
                    SECRET_MEMO,
                ],
                vec!["", "", "", "", "見せない列だけの値"],
                vec!["2026/10/21", "架空 次郎", "090-0000-0002", "", ""],
            ],
        )
        .with_tab(
            "別の顧客",
            9,
            vec![vec!["応募者名"], vec!["記入例"], vec![OTHER_APPLICANT]],
        );
    *f.modified.lock().unwrap() = "2026-10-26T00:00:00.000Z".into();
    Arc::new(f)
}

fn col(sheet: &str, header: &str, visible: bool, in_list: bool, order: u32) -> ColumnSetting {
    ColumnSetting {
        sheet_reg_id: sheet.into(),
        header: header.into(),
        label: String::new(),
        visible,
        writable: false,
        in_list,
        multiline: false,
        kind: if header == "電話番号" {
            ColumnKind::Phone
        } else {
            ColumnKind::Text
        },
        group: ColumnGroup::Summary,
        order,
    }
}

fn reg(id: &str, customer: &str, spreadsheet: &str, sheet_id: i64) -> SheetReg {
    SheetReg {
        id: id.into(),
        customer_id: customer.into(),
        spreadsheet_id: spreadsheet.into(),
        sheet_id,
        tab_title: String::new(),
        label: format!("{id} の一覧"),
        header_row: 1,
        example_rows: 1,
        customer_can_edit_columns: false,
        disabled: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        notify_tab: String::new(),
    }
}

struct H {
    app: Router,
    auth: Arc<super::super::auth::PortalAuth>,
    fake: Arc<FakeSheets>,
    /// 顧客 c1 の人
    a: Vec<String>,
    a_id: String,
    /// 顧客 c2 の人
    b: Vec<String>,
}

/// パスワードと確認コードでログインし、顧客の Cookie を返す (時刻は今)
fn sign_in(auth: &super::super::auth::PortalAuth, email: &str, customer: &str) -> (String, String) {
    let (a, token) = auth.create_account(email, customer, Utc::now()).unwrap();
    auth.accept_invite(&token, PW, Utc::now()).unwrap();
    let super::super::auth::LoginResult::CodeRequired {
        challenge, code, ..
    } = auth.login(email, PW, None, "1.1.1.1", Utc::now())
    else {
        panic!("コードを聞かない");
    };
    let super::super::auth::CodeResult::SignedIn { session, .. } =
        auth.verify_code(&challenge, &code, false, Utc::now())
    else {
        panic!("ログインできない");
    };
    (a.id, format!("{SESSION_COOKIE}={session}"))
}

fn harness() -> H {
    let store = store_c1();
    store
        .insert_customer(Customer {
            id: "c2".into(),
            name: "別の顧客".into(),
            hubspot_company_id: None,
            stage: CustomerStage::PortalOnly,
            disabled: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    store
        .insert_sheet(reg("s1", "c1", "SPREAD-c1-000000000000000", 777))
        .unwrap();
    store.replace_columns(
        "s1",
        vec![
            col("s1", "応募者名", true, true, 0),
            col("s1", "状況", true, true, 1),
            col("s1", "応募日", true, false, 2),
            col("s1", "電話番号", true, false, 3),
            col("s1", SECRET_HEADER, false, false, 4),
        ],
    );
    // 見せる項目の無いシートは一覧に出さない
    store
        .insert_sheet(reg("s1-empty", "c1", "SPREAD-c1-000000000000000", 778))
        .unwrap();
    store
        .insert_sheet(reg("s2", "c2", "SPREAD-c2-000000000000000", 9))
        .unwrap();
    store.replace_columns("s2", vec![col("s2", "応募者名", true, true, 0)]);
    // 社内用シートと同じスプレッドシートは、登録されていても読ませない
    store
        .insert_sheet(reg("s-store", "c1", STORE_SHEET, 1))
        .unwrap();
    store.replace_columns("s-store", vec![col("s-store", "応募者名", true, true, 0)]);

    let auth = Arc::new(super::super::auth::PortalAuth::new(store, cfg()));
    let (a_id, a) = sign_in(&auth, "a@example.com", "c1");
    let (_, b) = sign_in(&auth, "b@example.com", "c2");
    let fake = fake();
    let parts = Parts {
        auth: Some(auth.clone()),
        mailer: Some(Arc::new(FakeMailer::default())),
        reader: Some(fake.clone()),
        store_spreadsheet: Some(STORE_SHEET.into()),
        notify: None,
    };
    let app = router_parts(parts, CrmAccess::from_list(""), None).with_state(test_state());
    H {
        app,
        auth,
        fake,
        a: vec![a],
        a_id,
        b: vec![b],
    }
}

async fn get(h: &H, who: &[String], uri: &str) -> (StatusCode, Value) {
    let (s, _, v) = call(&h.app, "GET", uri, who, None, false).await;
    (s, v)
}

#[tokio::test(start_paused = true)]
async fn 自分の顧客のシートだけが一覧に出て_見せる項目だけを返す() {
    let h = harness();
    let (s, v) = get(&h, &h.a, "/api/portal/sheets").await;
    assert_eq!(s, StatusCode::OK);
    let ids: Vec<&str> = v["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec!["s1"],
        "見せる項目の無いシート・社内用シート・別の顧客は出さない"
    );
    assert_eq!(v["sheets"][0]["label"], "s1 の一覧");

    let (s, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::OK);
    let labels: Vec<&str> = v["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, vec!["応募者名", "状況", "応募日", "電話番号"]);
    assert_eq!(v["columns"][0]["in_list"], true);
    assert_eq!(v["columns"][2]["in_list"], false);
    assert_eq!(v["columns"][3]["kind"], "phone");
    // 記入例の行と、見せる項目がどれも空の行 (4 行目) は出さない。行番号はシートの行番号
    assert_eq!(
        v["applicants"],
        serde_json::json!([
            { "row": 3, "cells": [APPLICANT, "面接調整中", "2026/10/20", "090-0000-0001"] },
            { "row": 5, "cells": ["架空 次郎", "", "2026/10/21", "090-0000-0002"] },
        ])
    );
    assert_eq!(v["stale"], false);
    let text = v.to_string();
    assert!(!text.contains(SECRET_MEMO), "見せない項目の値が出ている");
    assert!(
        !text.contains(SECRET_HEADER),
        "見せない項目の見出しが出ている"
    );
    assert!(!text.contains("見せない列だけの値"));
    assert!(
        !text.contains("SPREAD-c1"),
        "スプレッドシートの番号を出さない"
    );
    // 閲覧の記録が 1 件残る (人の名前は入れない)
    let logs = h.auth.store().recent_logs(10);
    let views: Vec<_> = logs.iter().filter(|e| e.kind == "view").collect();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].account_id, h.a_id);
    assert_eq!(views[0].sheet_reg_id, "s1");
    assert!(!views[0].detail.contains(APPLICANT));
}

#[tokio::test(start_paused = true)]
async fn 別の顧客のシート_無い番号_社内用シートはどれも同じ404() {
    let h = harness();
    for path in ["rows", "version"] {
        let mut bodies = Vec::new();
        for id in ["s2", "nope", "s1-empty", "s-store"] {
            let (s, v) = get(&h, &h.a, &format!("/api/portal/sheets/{id}/{path}")).await;
            assert_eq!(s, StatusCode::NOT_FOUND, "{id} {path}");
            bodies.push(v);
        }
        assert!(
            bodies.windows(2).all(|w| w[0] == w[1]),
            "応答で存在が分かる: {bodies:?}"
        );
    }
    // 顧客 c2 の人は自分のシートは読めるが、c1 のシートは読めない
    let (s, v) = get(&h, &h.b, "/api/portal/sheets/s2/rows").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["applicants"][0]["cells"][0], OTHER_APPLICANT);
    let (s, _) = get(&h, &h.b, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // 404 の要求ではシートを読まない (別の顧客のシートを読みにいかない)
    let reads = h.fake.count("batch_get");
    assert_eq!(reads, 1, "c2 の自分のシートの 1 回だけ");
}

#[tokio::test(start_paused = true)]
async fn ログインしていない_止めたアカウント_止めた顧客は401() {
    let h = harness();
    for uri in [
        "/api/portal/sheets",
        "/api/portal/sheets/s1/rows",
        "/api/portal/sheets/s1/version",
    ] {
        assert_eq!(get(&h, &[], uri).await.0, StatusCode::UNAUTHORIZED, "{uri}");
    }
    assert_eq!(
        get(&h, &h.a, "/api/portal/sheets/s1/rows").await.0,
        StatusCode::OK
    );
    // アカウントを止めると、ログイン中の画面からも読めない
    h.auth.disable(&h.a_id, Utc::now()).unwrap();
    for uri in [
        "/api/portal/sheets",
        "/api/portal/sheets/s1/rows",
        "/api/portal/sheets/s1/version",
    ] {
        assert_eq!(
            get(&h, &h.a, uri).await.0,
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
    }
    // 顧客を止めると、その顧客の人は読めない
    let mut c2 = h.auth.store().customer("c2").unwrap();
    c2.disabled = true;
    h.auth.store().update_customer(c2).unwrap();
    assert_eq!(
        get(&h, &h.b, "/api/portal/sheets/s2/rows").await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(start_paused = true)]
async fn 最終更新時刻が変わらなければsheetsを読まず_変わったら1回だけ読む() {
    let h = harness();
    let (_, first) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(h.fake.count("tabs"), 1);
    assert_eq!(h.fake.count("batch_get"), 1);
    assert_eq!(h.fake.count("modified_time"), 1);
    let v0 = first["version"].as_str().unwrap().to_string();

    // 同じ時刻のうちは確認が何回来ても Drive は 1 回、Sheets は 0 回
    for _ in 0..5 {
        let (s, v) = get(&h, &h.a, "/api/portal/sheets/s1/version").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["version"], v0.as_str());
    }
    assert_eq!(
        h.fake.count("modified_time"),
        1,
        "20 秒の間は Drive を 1 回にまとめる"
    );
    // まとめる時間を過ぎても、時刻が同じなら Sheets は読まない
    tokio::time::advance(Duration::from_secs(21)).await;
    let (_, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(v["version"], v0.as_str());
    assert_eq!(h.fake.count("modified_time"), 2);
    assert_eq!(h.fake.count("batch_get"), 1, "時刻が同じなのに読んだ");

    // シートが書き換わる: 応募者が 1 人増え、Drive の時刻が進む
    h.fake.set_cell("応募者情報管理", 5, 1, "架空 三郎");
    *h.fake.modified.lock().unwrap() = "2026-10-26T00:05:00.000Z".into();
    tokio::time::advance(Duration::from_secs(21)).await;
    let (_, v) = get(&h, &h.a, "/api/portal/sheets/s1/version").await;
    let v1 = v["version"].as_str().unwrap().to_string();
    assert_ne!(v1, v0, "時刻が変われば版も変わる");
    assert_eq!(
        h.fake.count("batch_get"),
        1,
        "確認だけでは Sheets を読まない"
    );

    // 同時に 5 人が読み直しても、Sheets は 1 回だけ読む
    *h.fake.read_delay.lock().unwrap() = Some(Duration::from_millis(500));
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..5 {
        let app = h.app.clone();
        let who = h.a.clone();
        set.spawn(async move {
            call(&app, "GET", "/api/portal/sheets/s1/rows", &who, None, false).await
        });
    }
    let mut results = Vec::new();
    while let Some(r) = set.join_next().await {
        results.push(r.unwrap());
    }
    assert_eq!(h.fake.count("batch_get"), 2, "変わったあとに 1 回だけ読む");
    assert_eq!(h.fake.count("tabs"), 1, "タブの名前は 10 分使い回す");
    for (s, _, v) in &results {
        assert_eq!(*s, StatusCode::OK);
        assert_eq!(v["version"], v1.as_str());
        let names: Vec<&str> = v["applicants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["cells"][0].as_str().unwrap())
            .collect();
        assert_eq!(names, vec![APPLICANT, "架空 次郎", "架空 三郎"]);
    }
    // 閲覧の記録は 30 分に 1 回 (読み直しで増やさない)
    let views = h
        .auth
        .store()
        .recent_logs(50)
        .iter()
        .filter(|e| e.kind == "view")
        .count();
    assert_eq!(views, 1);
}

#[tokio::test(start_paused = true)]
async fn 項目の設定を変えると版が変わり_見せない項目にした値は出なくなる() {
    let h = harness();
    let (_, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    let v0 = v["version"].as_str().unwrap().to_string();
    assert!(v.to_string().contains("090-0000-0001"));
    let mut cols = h.auth.store().columns("s1");
    for c in cols.iter_mut() {
        if c.header == "電話番号" {
            c.visible = false;
            c.in_list = false;
        }
    }
    h.auth.store().replace_columns("s1", cols);
    let (_, v) = get(&h, &h.a, "/api/portal/sheets/s1/version").await;
    assert_ne!(v["version"].as_str().unwrap(), v0);
    let (_, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert!(
        !v.to_string().contains("090-0000-0001"),
        "見せない項目にした値が出ている"
    );
    assert_eq!(
        h.fake.count("batch_get"),
        1,
        "設定の変更だけでは Sheets を読み直さない"
    );
}

#[tokio::test(start_paused = true)]
async fn 確かめられないときは前の内容と時点を返し_初めてなら503() {
    let h = harness();
    *h.fake.fail_modified.lock().unwrap() = Some(GoogleError::Status(429));
    let (s, v) = get(&h, &h.a, "/api/portal/sheets/s1/version").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"], "sheet_busy");
    // Drive が止まっていても、初めて開くときは Sheets を読んで出す
    let (s, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["stale"], false);
    assert_eq!(h.fake.count("batch_get"), 1);
    // 2 回目は前の内容を、前の時点のまま返す (Sheets を叩き続けない)
    let (s, v2) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v2["stale"], true);
    assert_eq!(v2["read_at"], v["read_at"]);
    assert_eq!(h.fake.count("batch_get"), 1);
    // 共有が外れた (403) ら、前の内容も出さない
    *h.fake.fail_modified.lock().unwrap() = None;
    *h.fake.modified.lock().unwrap() = "2026-10-26T01:00:00.000Z".into();
    *h.fake.fail_reads.lock().unwrap() = Some(GoogleError::Status(403));
    tokio::time::advance(Duration::from_secs(21)).await;
    let (s, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"], "sheet_unavailable");
    assert!(!v.to_string().contains(APPLICANT));
}

#[tokio::test(start_paused = true)]
async fn 応答は保存させない() {
    let h = harness();
    for uri in [
        "/api/portal/sheets",
        "/api/portal/sheets/s1/rows",
        "/api/portal/sheets/s1/version",
    ] {
        let req = Request::get(uri)
            .header(header::COOKIE, h.a.join("; "))
            .body(axum::body::Body::empty())
            .unwrap();
        let res = tower::ServiceExt::oneshot(h.app.clone(), req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{uri}");
        assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store", "{uri}");
    }
}

#[tokio::test(start_paused = true)]
async fn 保存先とメールが揃っていなければ503() {
    let parts = Parts {
        auth: None,
        mailer: None,
        reader: Some(fake()),
        store_spreadsheet: None,
        notify: None,
    };
    let app = router_parts(parts, CrmAccess::from_list(""), None).with_state(test_state());
    for uri in ["/api/portal/sheets", "/api/portal/sheets/s1/rows"] {
        let (s, _, _) = call(&app, "GET", uri, &[], None, false).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
    }
}

#[tokio::test(start_paused = true)]
async fn driveが404_sheetsが403なら前の表を出さず503() {
    let h = harness();
    let (s, _) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::OK);
    // 共有が外れた: Drive は 404、Sheets は 403
    *h.fake.fail_modified.lock().unwrap() = Some(GoogleError::Status(404));
    *h.fake.fail_reads.lock().unwrap() = Some(GoogleError::Status(403));
    tokio::time::advance(Duration::from_secs(21)).await;
    for path in ["rows", "version"] {
        let (s, v) = get(&h, &h.a, &format!("/api/portal/sheets/s1/{path}")).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(v["error"], "sheet_unavailable", "{path}");
        assert!(!v.to_string().contains(APPLICANT), "{path}");
    }
    // Drive だけ戻っても (時刻は同じ)、捨てた表は出さず、Sheets の 403 をそのまま返す
    *h.fake.fail_modified.lock().unwrap() = None;
    tokio::time::advance(Duration::from_secs(21)).await;
    let (s, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"], "sheet_unavailable");
    assert!(!v.to_string().contains(APPLICANT));
    // 共有し直せば、また読める
    *h.fake.fail_reads.lock().unwrap() = None;
    tokio::time::advance(Duration::from_secs(11)).await;
    let (s, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["stale"], false);
}

#[tokio::test(start_paused = true)]
async fn 読み取りの失敗は10秒覚え_連打でsheetsを読まない() {
    let h = harness();
    get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(h.fake.count("batch_get"), 1);
    // シートが書き換わったが、Sheets が混んでいる (500)
    *h.fake.modified.lock().unwrap() = "2026-10-26T00:09:00.000Z".into();
    *h.fake.fail_reads.lock().unwrap() = Some(GoogleError::Status(500));
    tokio::time::advance(Duration::from_secs(21)).await;
    for _ in 0..10 {
        let (s, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
        assert_eq!(s, StatusCode::OK, "一時的な失敗は前の表を出す");
        assert_eq!(v["stale"], true);
    }
    assert_eq!(
        h.fake.count("batch_get"),
        2,
        "失敗のあと 10 秒は読みにいかない"
    );
    // 10 秒たてば読み直す
    *h.fake.fail_reads.lock().unwrap() = None;
    tokio::time::advance(Duration::from_secs(11)).await;
    let (_, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(v["stale"], false);
    assert_eq!(h.fake.count("batch_get"), 3);
}

#[tokio::test(start_paused = true)]
async fn 同じ見出しが2つある項目は出さない() {
    let h = harness();
    h.fake.tabs.lock().unwrap().insert(
        "重複".into(),
        (
            31,
            vec![
                vec!["応募者名".into(), "備考".into(), "備考".into()],
                vec!["記入例".into(), "".into(), "".into()],
                vec![APPLICANT.into(), "左の備考".into(), "右の備考".into()],
            ],
        ),
    );
    let store = h.auth.store();
    store
        .insert_sheet(reg("s3", "c1", "SPREAD-c1-dup-00000000000", 31))
        .unwrap();
    store.replace_columns(
        "s3",
        vec![
            col("s3", "応募者名", true, true, 0),
            col("s3", "備考", true, false, 1),
        ],
    );
    let (s, v) = get(&h, &h.a, "/api/portal/sheets/s3/rows").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["columns"].as_array().unwrap().len(), 1);
    assert_eq!(v["columns"][0]["label"], "応募者名");
    assert_eq!(v["applicants"][0]["cells"], serde_json::json!([APPLICANT]));
    let text = v.to_string();
    assert!(!text.contains("左の備考") && !text.contains("右の備考"));
}

#[tokio::test(start_paused = true)]
async fn 止めたシートの登録は404() {
    let h = harness();
    assert_eq!(
        get(&h, &h.a, "/api/portal/sheets/s1/rows").await.0,
        StatusCode::OK
    );
    let mut s1 = h.auth.store().sheet("s1").unwrap();
    s1.disabled = true;
    h.auth.store().update_sheet(s1).unwrap();
    for path in ["rows", "version"] {
        let (s, v) = get(&h, &h.a, &format!("/api/portal/sheets/s1/{path}")).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(v["error"], "not_found");
    }
    let (_, v) = get(&h, &h.a, "/api/portal/sheets").await;
    assert_eq!(v["sheets"], serde_json::json!([]));
}
