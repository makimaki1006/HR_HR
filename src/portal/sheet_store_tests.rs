//! 社内用シートの保存先のテスト (偽の Sheets)。
use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};

use super::auth::{AuthConfig, CodeResult, LoginResult, PortalAuth};
use super::sheet_store::*;
use super::sheets::GoogleError;
use super::store::*;
use super::test_fake::FakeSheets;

const PW: &str = "correct horse battery";

fn cfg() -> AuthConfig {
    let mut c = AuthConfig::new(5, 300);
    c.bcrypt_cost = 4;
    c
}

fn t0() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 23, 1, 0, 0).unwrap()
}

fn customer(id: &str) -> Customer {
    Customer {
        id: id.into(),
        name: "テスト顧客".into(),
        hubspot_company_id: Some("123".into()),
        stage: CustomerStage::Both,
        disabled: false,
        created_at: t0(),
        updated_at: t0(),
    }
}

async fn loaded(fake: &Arc<FakeSheets>) -> Arc<SheetStore> {
    let s = SheetStore::new(fake.clone(), "store-sheet", SheetStoreConfig::default());
    s.load().await.expect("読み込める");
    s
}

#[tokio::test]
async fn 空のシートにタブと見出しを作ってから使えるようにする() {
    let fake = Arc::new(FakeSheets::default());
    let s = SheetStore::new(fake.clone(), "store-sheet", SheetStoreConfig::default());
    assert!(!s.ready(), "読み込む前は使えない");
    s.load().await.unwrap();
    assert!(s.ready());
    for (tab, headers) in [
        (TAB_CUSTOMERS, H_CUSTOMERS),
        (TAB_SHEETS, H_SHEETS),
        (TAB_COLUMNS, H_COLUMNS),
        (TAB_ACCOUNTS, H_ACCOUNTS),
        (TAB_INVITES, H_INVITES),
        (TAB_DEVICES, H_DEVICES),
        (TAB_SESSIONS, H_SESSIONS),
        (TAB_LOG, H_LOG),
    ] {
        assert_eq!(fake.rows(tab)[0], headers.to_vec(), "{tab} の見出し");
    }
}

/// 書いた内容を、別のプロセス (再起動のあと) が読み込んで同じように使えること
#[tokio::test]
async fn 書いた内容は再起動のあとも同じに使えて_シートにはハッシュだけを置く() {
    let fake = Arc::new(FakeSheets::default());
    let s1 = loaded(&fake).await;
    s1.insert_customer(customer("c1")).unwrap();
    let auth1 = PortalAuth::new(s1.clone(), cfg());
    let (account, token) = auth1
        .create_account("hanako@customer.example", "c1", t0())
        .unwrap();
    auth1.accept_invite(&token, PW, t0()).unwrap();
    let LoginResult::CodeRequired {
        challenge, code, ..
    } = auth1.login("hanako@customer.example", PW, None, "ip", t0())
    else {
        panic!("新しい端末なのにコードを聞かない")
    };
    let CodeResult::SignedIn { session, device } = auth1.verify_code(&challenge, &code, true, t0())
    else {
        panic!()
    };
    let device = device.unwrap();
    s1.append_log(LogEntry::new(
        t0(),
        "staff@f-a-c.co.jp",
        LogSource::Admin,
        "test",
    ));
    s1.flush_once().await.unwrap();
    assert_eq!(s1.status().pending, 0);

    // シートの中: 生の招待・端末・ログインの値とパスワードは無い
    let text = fake.all_text();
    for secret in [&token, &device, &session, &challenge, &code.to_string()] {
        assert!(!text.contains(secret.as_str()), "生の値がシートにある");
    }
    assert!(!text.contains(PW));
    assert!(text.contains("$2"), "パスワードは bcrypt のハッシュだけ");
    assert_eq!(fake.rows(TAB_LOG).len(), 2, "記録は見出しの下に 1 行");

    // 再起動: 新しい保存先が同じシートを読む
    let s2 = loaded(&fake).await;
    let auth2 = PortalAuth::new(s2.clone(), cfg());
    assert_eq!(
        s2.customer("c1"),
        s1.customer("c1").map(|mut c| {
            // 時刻はミリ秒までで保存する
            c.created_at = t0();
            c.updated_at = t0();
            c
        })
    );
    let a2 = s2.account(&account.id).unwrap();
    assert_eq!(a2.email, "hanako@customer.example");
    assert_eq!(a2.status, AccountStatus::Active);
    assert!(
        auth2.current(&session, t0()).is_some(),
        "ログイン中の画面が戻る"
    );
    assert!(
        matches!(
            auth2.login("hanako@customer.example", PW, Some(&device), "ip", t0()),
            LoginResult::SignedIn { .. }
        ),
        "覚えた端末が戻る"
    );
    assert!(
        auth2.invite_info(&token, t0()).is_err(),
        "使った招待は使えないまま"
    );
    // 記録は起動時に読まない
    assert!(s2.recent_logs(10).is_empty());
}

#[tokio::test]
async fn 書けなければ保存待ちに残し_直ったら書く_停止はすぐ効く() {
    let fake = Arc::new(FakeSheets::default());
    let s = loaded(&fake).await;
    s.insert_customer(customer("c1")).unwrap();
    let auth = PortalAuth::new(s.clone(), cfg());
    let (account, token) = auth.create_account("a@example.com", "c1", t0()).unwrap();
    auth.accept_invite(&token, PW, t0()).unwrap();
    s.flush_once().await.unwrap();
    let LoginResult::CodeRequired {
        challenge, code, ..
    } = auth.login("a@example.com", PW, None, "ip", t0())
    else {
        panic!()
    };
    let CodeResult::SignedIn { session, .. } = auth.verify_code(&challenge, &code, true, t0())
    else {
        panic!()
    };

    *fake.fail_writes.lock().unwrap() = Some(GoogleError::Status(503));
    auth.disable(&account.id, t0()).unwrap();
    assert!(
        auth.current(&session, t0()).is_none(),
        "シートに書けなくても停止はすぐ効く"
    );
    assert!(s.flush_once().await.is_err());
    let st = s.status();
    assert!(st.pending > 0, "保存待ちに残る");
    assert!(st.last_error.as_deref().unwrap().contains("503"));
    assert!(
        !s.persisted(Duration::from_millis(50)).await,
        "書けていないのに書けたと言わない"
    );
    // シートにはまだ反映されていない
    assert_eq!(
        loaded(&fake).await.account(&account.id).unwrap().status,
        AccountStatus::Active
    );

    *fake.fail_writes.lock().unwrap() = None;
    s.flush_once().await.unwrap();
    assert_eq!(s.status().pending, 0);
    assert!(s.status().last_error.is_none());
    let after = loaded(&fake).await;
    assert_eq!(
        after.account(&account.id).unwrap().status,
        AccountStatus::Disabled
    );
    let auth_after = PortalAuth::new(after, cfg());
    assert!(
        auth_after.current(&session, t0()).is_none(),
        "再起動のあとも止まったまま"
    );
}

#[tokio::test]
async fn 行を並べ替えられても鍵で引き直して正しい行に書く() {
    let fake = Arc::new(FakeSheets::default());
    let s = loaded(&fake).await;
    s.insert_customer(customer("c1")).unwrap();
    s.insert_customer(Customer {
        name: "別の顧客".into(),
        ..customer("c2")
    })
    .unwrap();
    s.flush_once().await.unwrap();
    // 人が 2 行を入れ替え、間に空の行を入れた
    {
        let mut g = fake.tabs.lock().unwrap();
        let rows = &mut g.get_mut(TAB_CUSTOMERS).unwrap().1;
        rows.swap(1, 2);
        rows.insert(1, Vec::new());
    }
    let mut c1 = s.customer("c1").unwrap();
    c1.disabled = true;
    s.update_customer(c1).unwrap();
    s.flush_once().await.unwrap();
    let rows = fake.rows(TAB_CUSTOMERS);
    let row_of = |id: &str| {
        rows.iter()
            .find(|r| r.first().map(String::as_str) == Some(id))
            .unwrap()
            .clone()
    };
    assert_eq!(row_of("c1")[4], "TRUE");
    assert_eq!(row_of("c2")[4], "FALSE");
    assert_eq!(row_of("c2")[1], "別の顧客");
    assert_eq!(rows.len(), 4, "行を増やさない");
}

#[tokio::test]
async fn 読み直しでシートの手直しを取り込み_保存待ちはメモリを残す() {
    let fake = Arc::new(FakeSheets::default());
    let s = loaded(&fake).await;
    s.insert_customer(customer("c1")).unwrap();
    s.insert_customer(customer("c2")).unwrap();
    s.insert_customer(customer("c3")).unwrap();
    s.flush_once().await.unwrap();
    let row = |id: &str| {
        fake.rows(TAB_CUSTOMERS)
            .iter()
            .position(|r| r.first().map(String::as_str) == Some(id))
            .unwrap()
    };
    // 社内の人がシートで c1 を止め、c2 の行を消した
    fake.set_cell(TAB_CUSTOMERS, row("c1"), 4, "TRUE");
    let r2 = row("c2");
    fake.tabs.lock().unwrap().get_mut(TAB_CUSTOMERS).unwrap().1[r2] = Vec::new();
    // c3 はアプリで名前を変えたが、まだ書けていない
    *fake.fail_writes.lock().unwrap() = Some(GoogleError::Network);
    let mut c3 = s.customer("c3").unwrap();
    c3.name = "名前を変えた".into();
    s.update_customer(c3).unwrap();
    assert!(s.flush_once().await.is_err());

    s.reload().await.unwrap();
    assert!(
        s.customer("c1").unwrap().disabled,
        "シートの手直しを取り込む"
    );
    assert!(s.customer("c2").is_none(), "シートから消えた行は消える");
    assert_eq!(
        s.customer("c3").unwrap().name,
        "名前を変えた",
        "保存待ちはメモリの方"
    );
    *fake.fail_writes.lock().unwrap() = None;
    s.flush_once().await.unwrap();
    assert_eq!(fake.rows(TAB_CUSTOMERS)[row("c3")][1], "名前を変えた");
}

#[tokio::test]
async fn 読めない値は閉じる側に倒す() {
    let ts = "2026-10-23T10:00:00.000+09:00";
    let fake = Arc::new(
        FakeSheets::default()
            .with_tab(
                TAB_CUSTOMERS,
                1,
                vec![
                    H_CUSTOMERS.to_vec(),
                    vec!["c1", "顧客", "", "おかしな段階", "", ts, ts],
                ],
            )
            .with_tab(
                TAB_ACCOUNTS,
                2,
                vec![
                    H_ACCOUNTS.to_vec(),
                    vec![
                        "a1",
                        "A@Example.com",
                        "c1",
                        "$2b$04$x",
                        "",
                        "おかしな状態",
                        "",
                        "",
                        ts,
                    ],
                    vec![
                        "a2",
                        "b@example.com",
                        "c1",
                        "$2b$04$x",
                        "",
                        "active",
                        "読めない日時",
                        "",
                        ts,
                    ],
                ],
            )
            .with_tab(
                TAB_INVITES,
                3,
                vec![
                    H_INVITES.to_vec(),
                    vec!["h1", "a1", "initial", "読めない期限", "", ""],
                    vec!["h2", "a1", "initial", ts, "", "x"],
                    vec!["h3", "a1", "initial", ts, "読めない", ""],
                ],
            )
            .with_tab(
                TAB_SESSIONS,
                4,
                vec![H_SESSIONS.to_vec(), vec!["s1", "a2", ts, "読めない"]],
            )
            .with_tab(
                TAB_COLUMNS,
                5,
                vec![
                    H_COLUMNS.to_vec(),
                    vec!["s-1", "備考", "", "いいえ", "TRUE", "", "", "謎", "謎", ""],
                ],
            ),
    );
    let s = loaded(&fake).await;
    assert_eq!(s.customer("c1").unwrap().stage, CustomerStage::SheetOnly);
    assert_eq!(s.account("a1").unwrap().status, AccountStatus::Disabled);
    assert_eq!(s.account("a1").unwrap().email, "a@example.com");
    assert_eq!(
        s.account("a2").unwrap().password_changed_at,
        Some(chrono::DateTime::<Utc>::MAX_UTC),
        "読めない変更日時は、それより前のログインを全部無効にする"
    );
    assert!(s.invite("h1").is_none(), "期限が読めない招待は捨てる");
    assert!(
        s.invite("h2").unwrap().revoked,
        "取り消しの列に何か入っていれば取り消し"
    );
    assert!(
        s.invite("h3").unwrap().used_at.is_some(),
        "読めない使用日時は使用済み"
    );
    assert!(s.session("s1").unwrap().ended_at.is_some());
    let col = &s.columns("s-1")[0];
    assert!(!col.visible && !col.writable, "見せない列は書かせない");
    assert_eq!(
        (col.kind, col.group),
        (ColumnKind::Text, ColumnGroup::Other)
    );
}

#[tokio::test]
async fn 見出しが違えば読み込まず_理由を出す() {
    let mut wrong = H_ACCOUNTS.to_vec();
    wrong.swap(1, 2);
    let fake = Arc::new(FakeSheets::default().with_tab(TAB_ACCOUNTS, 1, vec![wrong]));
    let s = SheetStore::new(fake.clone(), "store-sheet", SheetStoreConfig::default());
    assert_eq!(s.load().await, Err(LoadError::Headers(TAB_ACCOUNTS)));
    assert!(!s.ready());
    assert!(s.status().last_error.unwrap().contains("アカウント"));
    // 古い版の見出し (先頭だけ) なら足りない見出しを書き足して使う
    let fake =
        Arc::new(FakeSheets::default().with_tab(TAB_ACCOUNTS, 1, vec![H_ACCOUNTS[..5].to_vec()]));
    let s = loaded(&fake).await;
    assert!(s.ready());
    assert_eq!(fake.rows(TAB_ACCOUNTS)[0], H_ACCOUNTS.to_vec());
}

#[tokio::test]
async fn 読み込みに失敗したら使えないまま理由を出す() {
    let fake = Arc::new(FakeSheets::default());
    *fake.fail_reads.lock().unwrap() = Some(GoogleError::Status(403));
    let s = SheetStore::new(fake, "store-sheet", SheetStoreConfig::default());
    assert!(s.load().await.is_err());
    assert!(!s.ready());
    assert!(s.status().last_error.unwrap().contains("共有"));
}

#[tokio::test]
async fn 項目の設定を置き換えると消えた項目の行は空にする() {
    let fake = Arc::new(FakeSheets::default());
    let s = loaded(&fake).await;
    let col = |h: &str, order: u32| ColumnSetting {
        sheet_reg_id: "s-1".into(),
        header: h.into(),
        label: String::new(),
        visible: true,
        writable: false,
        in_list: true,
        multiline: false,
        kind: ColumnKind::Text,
        group: ColumnGroup::Summary,
        order,
    };
    s.replace_columns("s-1", vec![col("氏名", 0), col("状況", 1)]);
    s.flush_once().await.unwrap();
    s.replace_columns("s-1", vec![col("状況", 0)]);
    s.flush_once().await.unwrap();
    let again = loaded(&fake).await;
    let cols = again.columns("s-1");
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].header, "状況");
}

#[tokio::test]
async fn 裏の処理が書き終えるのを待てる_回数は変更をまとめた分だけ() {
    let fake = Arc::new(FakeSheets::default());
    let cfg = SheetStoreConfig {
        debounce: Duration::from_millis(20),
        min_gap: Duration::from_millis(20),
        reload_every: Duration::from_secs(3600),
        retry_max: Duration::from_millis(100),
    };
    let s = SheetStore::start(fake.clone(), "store-sheet", cfg);
    for _ in 0..100 {
        if s.ready() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(s.ready());
    let before = fake.calls.lock().unwrap().len();
    for i in 0..20 {
        s.insert_customer(customer(&format!("c{i}"))).unwrap();
    }
    assert!(s.persisted(Duration::from_secs(5)).await);
    assert_eq!(fake.rows(TAB_CUSTOMERS).len(), 21);
    let calls: Vec<String> = fake.calls.lock().unwrap()[before..].to_vec();
    assert_eq!(
        calls,
        vec!["batch_get", "batch_update"],
        "20 件の変更を、鍵の読み直し 1 回と書き込み 1 回にまとめる"
    );
    // 書けない間は待ってもだめと返し、直れば裏で再試行して書く
    *fake.fail_writes.lock().unwrap() = Some(GoogleError::Status(500));
    s.insert_customer(customer("late")).unwrap();
    assert!(!s.persisted(Duration::from_millis(200)).await);
    *fake.fail_writes.lock().unwrap() = None;
    assert!(s.persisted(Duration::from_secs(5)).await);
    assert!(loaded(&fake).await.customer("late").is_some());
}
