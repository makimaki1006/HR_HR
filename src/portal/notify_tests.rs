//! 応募の通知 (計画 14 章) のテスト。偽の Sheets / Drive / Mailer で動かす。値はすべて架空 (example.com)。
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;

use super::auth::{AuthConfig, PortalAuth};
use super::live::{LiveConfig, LiveSheets};
use super::mail::{MailError, MailFuture, Mailer, SendLimits};
use super::notify::*;
use super::sheet_store::{SheetStore, SheetStoreConfig, TAB_NOTIFIED};
use super::store::*;
use super::test_fake::FakeSheets;

const SPREAD: &str = "SPREAD-c1-000000000000000";
const TAB: &str = "応募者情報管理";
const TAB_TO: &str = "通知の送り先";
const URL: &str = "https://portal.example.com/portal";
const NAME: &str = "架空 花子";
const PHONE: &str = "090-0000-0001";
const MAIL: &str = "hanako@applicant.example.com";

#[derive(Default)]
struct FakeMailer {
    /// (宛先, 件名, 本文)
    sent: Mutex<Vec<(Vec<String>, String, String)>>,
    not_ready: Mutex<bool>,
    fail: Mutex<bool>,
}

impl FakeMailer {
    fn sent(&self) -> Vec<(Vec<String>, String, String)> {
        self.sent.lock().unwrap().clone()
    }
}

impl Mailer for FakeMailer {
    fn send<'a>(&'a self, to: &'a [String], subject: &'a str, body: &'a str) -> MailFuture<'a> {
        Box::pin(async move {
            if *self.fail.lock().unwrap() {
                return Err(MailError);
            }
            self.sent
                .lock()
                .unwrap()
                .push((to.to_vec(), subject.to_string(), body.to_string()));
            Ok(to.to_vec())
        })
    }
    fn ready(&self) -> bool {
        !*self.not_ready.lock().unwrap()
    }
}

fn cfg() -> AuthConfig {
    let mut c = AuthConfig::new(5, 300);
    c.bcrypt_cost = 4;
    c
}

fn applicants() -> Vec<Vec<&'static str>> {
    vec![
        vec!["応募日", "応募者名", "電話番号", "メールアドレス", "状況"],
        vec![
            "記入例",
            "記入例 太郎",
            "090-0000-0000",
            "rei@example.com",
            "",
        ],
        vec![
            "2026/10/20",
            "架空 一郎",
            "090-0000-0101",
            "ichiro@applicant.example.com",
            "未連絡",
        ],
        vec!["2026/10/21", "架空 次郎", "090-0000-0102", "", "面接調整中"],
    ]
}

/// 形のばらばらな送り先のタブ (列の並びに頼らずに拾うことの確かめ用)
fn recipients_tab() -> Vec<Vec<&'static str>> {
    vec![
        vec!["応募の通知の送り先"],
        vec!["部署", "担当", "メール"],
        vec!["採用", "採用の担当", "saiyo@example.com"],
        vec!["", "店舗: tenpo@example.com, tenpo-sub@example.com"],
        vec!["", "", "", "SAIYO@example.com"],
        vec!["メモ", "古い宛先 old@ は使わない"],
    ]
}

fn fake() -> Arc<FakeSheets> {
    let f = FakeSheets::default()
        .with_tab(TAB, 777, applicants())
        .with_tab(TAB_TO, 9, recipients_tab());
    *f.modified.lock().unwrap() = "2026-10-26T00:00:00.000Z".into();
    Arc::new(f)
}

fn col(header: &str, kind: ColumnKind, visible: bool, order: u32) -> ColumnSetting {
    ColumnSetting {
        sheet_reg_id: "s1".into(),
        header: header.into(),
        label: String::new(),
        visible,
        writable: false,
        in_list: false,
        multiline: false,
        kind,
        group: ColumnGroup::Summary,
        order,
    }
}

fn setup_store(store: &dyn PortalStore, notify_tab: &str) {
    store
        .insert_customer(Customer {
            id: "c1".into(),
            name: "架空運送株式会社".into(),
            hubspot_company_id: None,
            stage: CustomerStage::Both,
            disabled: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    store
        .insert_sheet(SheetReg {
            id: "s1".into(),
            customer_id: "c1".into(),
            spreadsheet_id: SPREAD.into(),
            sheet_id: 777,
            tab_title: TAB.into(),
            label: "応募者一覧".into(),
            header_row: 1,
            example_rows: 1,
            customer_can_edit_columns: false,
            disabled: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            notify_tab: notify_tab.into(),
        })
        .unwrap();
    // メールアドレスの項目は「見せない」でも指紋に使う
    store.replace_columns(
        "s1",
        vec![
            col("応募日", ColumnKind::Date, true, 0),
            col("応募者名", ColumnKind::Text, true, 1),
            col("電話番号", ColumnKind::Phone, true, 2),
            col("メールアドレス", ColumnKind::Email, false, 3),
            col("状況", ColumnKind::Choice, true, 4),
        ],
    );
}

fn live(f: &Arc<FakeSheets>) -> Arc<LiveSheets> {
    Arc::new(LiveSheets::new(
        f.clone(),
        LiveConfig {
            version_ttl: Duration::ZERO,
            error_ttl: Duration::ZERO,
            tabs_ttl: Duration::from_secs(600),
            idle_drop: Duration::from_secs(900),
        },
    ))
}

fn notifier(
    store: Arc<dyn PortalStore>,
    f: &Arc<FakeSheets>,
    m: &Arc<FakeMailer>,
) -> Arc<Notifier> {
    Notifier::new(
        Arc::new(PortalAuth::new(store, cfg())),
        m.clone(),
        live(f),
        Arc::new(SendLimits::default()),
        derive_key(b"test-secret"),
        URL.into(),
        None,
        NotifyConfig::default(),
    )
}

fn memory(notify_tab: &str) -> Arc<MemoryStore> {
    let s = Arc::new(MemoryStore::default());
    setup_store(s.as_ref(), notify_tab);
    s
}

/// シートの下に応募者を 1 人足し、Drive の最終更新時刻を進める
fn add_applicant(f: &FakeSheets, at: &str, cells: Vec<&str>) {
    let n = f.rows(TAB).len();
    for (i, v) in cells.iter().enumerate() {
        f.set_cell(TAB, n, i, v);
    }
    *f.modified.lock().unwrap() = at.into();
}

fn touch(f: &FakeSheets, at: &str) {
    *f.modified.lock().unwrap() = at.into();
}

#[test]
fn hmacは_rfc4231の例と同じ() {
    let h = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn 送り先のタブの形がばらばらでもアドレスを拾う() {
    let cells: Vec<Vec<String>> = recipients_tab()
        .into_iter()
        .map(|r| r.into_iter().map(str::to_string).collect())
        .chain([vec![
            "mailto:Mixed.Case@Example.com".to_string(),
            "<bracket@example.com>；全角の区切り＠example.com 「kakko@example.com」".to_string(),
        ]])
        .collect();
    let (addrs, skipped) = super::notify::extract_addresses(&cells);
    assert_eq!(
        addrs,
        vec![
            "saiyo@example.com",
            "tenpo@example.com",
            "tenpo-sub@example.com",
            "mixed.case@example.com",
            "bracket@example.com",
            "kakko@example.com",
        ],
        "重なり (大文字小文字違い) は 1 回、左上から順"
    );
    assert_eq!(skipped, 2, "old@ と全角の＠を飛ばした数");
}

#[test]
fn 本文に個人情報を入れない() {
    let (subject, body) = notify_message("架空運送株式会社", URL);
    assert_eq!(subject, "新しい応募があります（架空運送株式会社）");
    assert!(body.contains(URL));
    for secret in [NAME, PHONE, MAIL, "090", "@"] {
        assert!(!body.contains(secret), "本文に {secret} が無い");
        assert!(!subject.contains(secret));
    }
    let (s, _) = notify_message("改行\nのある名前", URL);
    assert!(!s.contains('\n'), "件名に改行を入れない");
}

#[tokio::test]
async fn 初回は送らず_新しい行で送り先全員に1通ずつ_同じ行は2度送らない() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(TAB_TO);
    let n = notifier(store.clone(), &f, &m);

    n.tick().await;
    assert!(
        m.sent().is_empty(),
        "初めて読むときは今ある行を基準にするだけ"
    );
    let st = n.status("s1");
    assert_eq!(st.recipients, Some(3), "読めた送り先の数");
    assert_eq!(st.skipped, 1, "形の正しくないもの (old@) の数");
    assert_eq!(st.problem, None);

    // 変わっていなければ Sheets を読まない
    let reads = f.count("batch_get");
    n.tick().await;
    assert_eq!(
        f.count("batch_get"),
        reads,
        "最終更新時刻が同じなら読まない"
    );

    add_applicant(
        &f,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE, MAIL, "未連絡"],
    );
    n.tick().await;
    let sent = m.sent();
    assert_eq!(sent.len(), 1, "1 回の確認で 1 回送る");
    let (to, subject, body) = &sent[0];
    assert_eq!(
        to,
        &vec![
            "saiyo@example.com".to_string(),
            "tenpo@example.com".into(),
            "tenpo-sub@example.com".into()
        ],
        "送り先のタブの全員 (宛先ごとに 1 通ずつ送る Mailer に渡す)"
    );
    assert!(subject.contains("架空運送株式会社"));
    for secret in [NAME, PHONE, MAIL, "0001"] {
        assert!(
            !body.contains(secret) && !subject.contains(secret),
            "{secret}"
        );
    }
    assert!(body.contains(URL));
    let st = n.status("s1");
    assert!(st.last_sent_at.is_some());
    assert_eq!(st.waiting, 0);

    // 記録: 行番号と通数。指紋は残さない
    let log = store
        .recent_logs(10)
        .into_iter()
        .find(|e| e.kind == "notify")
        .expect("送った記録");
    assert_eq!(log.row, "5");
    assert!(log.detail.contains("3 通"), "{}", log.detail);
    assert_eq!(store.notified("s1").len(), 1);
    let fp = &store.notified("s1")[0].fingerprint;
    assert!(!log.detail.contains(fp.as_str()));
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()) && !fp.contains("0001"));

    // 状況の書き換え・並べ替え・電話の書き換えでは送らない
    f.set_cell(TAB, 4, 4, "面接調整中");
    touch(&f, "2026-10-26T02:00:00.000Z");
    n.tick().await;
    {
        let mut g = f.tabs.lock().unwrap();
        let rows = &mut g.get_mut(TAB).unwrap().1;
        rows.swap(2, 4);
    }
    touch(&f, "2026-10-26T03:00:00.000Z");
    n.tick().await;
    f.set_cell(TAB, 3, 2, "090-0000-0199");
    touch(&f, "2026-10-26T04:00:00.000Z");
    n.tick().await;
    assert_eq!(m.sent().len(), 1, "新しい行でなければ送らない");

    // 途中に行を入れた新しい応募は送る
    {
        let mut g = f.tabs.lock().unwrap();
        let rows = &mut g.get_mut(TAB).unwrap().1;
        rows.insert(
            2,
            ["2026/10/27", "架空 三郎", "090-0000-0103", "", "未連絡"]
                .map(str::to_string)
                .to_vec(),
        );
    }
    touch(&f, "2026-10-26T05:00:00.000Z");
    n.tick().await;
    assert_eq!(m.sent().len(), 2, "途中に入れた新しい行");
    let log = store
        .recent_logs(10)
        .into_iter()
        .find(|e| e.kind == "notify")
        .unwrap();
    assert_eq!(log.row, "3");
}

#[tokio::test]
async fn 再起動を挟んでも同じ行には送らない() {
    let customer = fake();
    let internal = Arc::new(FakeSheets::default());
    let m = Arc::new(FakeMailer::default());

    let s1 = SheetStore::new(internal.clone(), "store-sheet", SheetStoreConfig::default());
    s1.load().await.unwrap();
    setup_store(s1.as_ref(), TAB_TO);
    let n1 = notifier(s1.clone(), &customer, &m);
    n1.tick().await;
    add_applicant(
        &customer,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE, MAIL, "未連絡"],
    );
    n1.tick().await;
    assert_eq!(m.sent().len(), 1);
    s1.flush_once().await.unwrap();
    let saved = internal.rows(TAB_NOTIFIED);
    assert_eq!(saved.len(), 2, "見出しと送った行 1 つ");
    let text = internal.all_text();
    for secret in [NAME, PHONE, "0900000", MAIL] {
        assert!(!text.contains(secret), "社内用シートに {secret} が無い");
    }

    // 再起動: 社内用シートから読み直す。その間に行が一時的に消えていた (切り取って貼り直す途中など)
    let s2 = SheetStore::new(internal.clone(), "store-sheet", SheetStoreConfig::default());
    s2.load().await.unwrap();
    assert_eq!(s2.notified("s1").len(), 1, "送った行を読み直した");
    let moved = customer.rows(TAB).pop().unwrap();
    customer.tabs.lock().unwrap().get_mut(TAB).unwrap().1.pop();
    touch(&customer, "2026-10-26T02:00:00.000Z");
    let n2 = notifier(s2.clone(), &customer, &m);
    n2.tick().await;
    assert_eq!(m.sent().len(), 1, "再起動の直後は基準を作るだけ");
    customer
        .tabs
        .lock()
        .unwrap()
        .get_mut(TAB)
        .unwrap()
        .1
        .push(moved);
    touch(&customer, "2026-10-26T03:00:00.000Z");
    n2.tick().await;
    assert_eq!(
        m.sent().len(),
        1,
        "前に送った行は、再起動を挟んでも送らない"
    );

    add_applicant(
        &customer,
        "2026-10-26T04:00:00.000Z",
        vec!["2026/10/28", "架空 四郎", "090-0000-0104", "", ""],
    );
    n2.tick().await;
    assert_eq!(m.sent().len(), 2, "本当に新しい行は送る");
}

#[tokio::test]
async fn 送信の許可が無ければ送らず_戻ったらまとめて1通で知らせる() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    *m.not_ready.lock().unwrap() = true;
    let store = memory(TAB_TO);
    let n = notifier(store.clone(), &f, &m);
    n.tick().await;
    assert_eq!(f.count("batch_get"), 0, "許可が無い間はシートを読まない");
    assert_eq!(n.status("s1").problem, Some(Problem::MailNotReady));

    *m.not_ready.lock().unwrap() = false;
    n.tick().await;
    *m.not_ready.lock().unwrap() = true;
    add_applicant(
        &f,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE, MAIL],
    );
    add_applicant(
        &f,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", "架空 五郎", "090-0000-0105"],
    );
    n.tick().await;
    assert!(m.sent().is_empty(), "許可が無ければ送らない");

    // 送れなかった (送信の失敗) 行は次の確認で送る
    *m.not_ready.lock().unwrap() = false;
    *m.fail.lock().unwrap() = true;
    n.tick().await;
    assert!(m.sent().is_empty());
    let st = n.status("s1");
    assert_eq!(st.problem, Some(Problem::MailFailed));
    assert_eq!(st.waiting, 2);
    assert!(
        store.notified("s1").is_empty(),
        "送れなかった行は「送った」にしない"
    );

    *m.fail.lock().unwrap() = false;
    n.tick().await;
    assert_eq!(m.sent().len(), 1, "2 行をまとめて 1 回");
    assert_eq!(store.notified("s1").len(), 2);
    n.tick().await;
    assert_eq!(m.sent().len(), 1);
}

#[tokio::test]
async fn 送り先のタブが無い_空_応募者のタブなら送らずに印を出す() {
    let f = fake();
    f.tabs
        .lock()
        .unwrap()
        .insert("空のタブ".into(), (10, vec![vec!["送り先".into()]]));
    let m = Arc::new(FakeMailer::default());
    for (tab, want) in [
        ("無いタブ", Problem::TabMissing("無いタブ".into())),
        ("空のタブ", Problem::NoRecipients),
        (TAB, Problem::ApplicantTab),
    ] {
        let store = memory(tab);
        let n = notifier(store.clone(), &f, &m);
        n.tick().await;
        add_applicant(
            &f,
            &format!("2026-10-26T01:{:02}:00.000Z", f.rows(TAB).len()),
            vec!["2026/10/26", NAME, PHONE, MAIL],
        );
        n.tick().await;
        assert!(m.sent().is_empty(), "{tab}: 送らない");
        let st = n.status("s1");
        assert_eq!(st.problem, Some(want), "{tab}");
        assert_eq!(st.waiting, 1, "{tab}: 直したら送る");
    }
}

#[tokio::test]
async fn 指紋に使う項目を変えても一斉には送らない() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(TAB_TO);
    let n = notifier(store.clone(), &f, &m);
    n.tick().await;
    let mut cols = store.columns("s1");
    for c in &mut cols {
        if c.header == "メールアドレス" {
            c.kind = ColumnKind::Text;
        }
    }
    store.replace_columns("s1", cols);
    n.tick().await;
    assert!(m.sent().is_empty(), "設定を変えた直後は基準を作り直すだけ");
}

#[tokio::test]
async fn 止めた顧客と止めたシートには送らない() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(TAB_TO);
    let n = notifier(store.clone(), &f, &m);
    n.tick().await;
    let mut c = store.customer("c1").unwrap();
    c.disabled = true;
    store.update_customer(c).unwrap();
    add_applicant(
        &f,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE, MAIL],
    );
    n.tick().await;
    assert!(m.sent().is_empty());
    assert_eq!(n.status("s1").problem, Some(Problem::CustomerUnusable));

    let mut c = store.customer("c1").unwrap();
    c.disabled = false;
    store.update_customer(c).unwrap();
    let mut s = store.sheet("s1").unwrap();
    s.disabled = true;
    store.update_sheet(s).unwrap();
    n.tick().await;
    assert!(m.sent().is_empty());
    assert_eq!(n.status("s1").problem, Some(Problem::SheetStopped));
}
