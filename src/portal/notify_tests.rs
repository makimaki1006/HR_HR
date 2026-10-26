//! 応募の通知 (計画 14 章) のテスト。偽の Sheets / Drive / Mailer で動かす。値はすべて架空 (example.com)。
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;

use super::auth::{AuthConfig, PortalAuth};
use super::live::{LiveConfig, LiveSheets};
use super::mail::{MailError, MailFuture, Mailer};
use super::notify::*;
use super::sheet_store::{SheetStore, SheetStoreConfig, TAB_NOTIFIED};
use super::store::*;
use super::test_fake::FakeSheets;

const SPREAD: &str = "SPREAD-c1-000000000000000";
const TAB: &str = "応募者情報管理";
const TAB_ID: i64 = 777;
const TAB_TO: &str = "通知の送り先";
const TAB_TO_ID: i64 = 9;
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
    /// 届かない宛先
    reject: Mutex<Vec<String>>,
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
            let reject = self.reject.lock().unwrap().clone();
            let ok: Vec<String> = to.iter().filter(|a| !reject.contains(a)).cloned().collect();
            if ok.is_empty() {
                return Err(MailError);
            }
            Ok(ok)
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

const ALL_TO: [&str; 3] = [
    "saiyo@example.com",
    "tenpo@example.com",
    "tenpo-sub@example.com",
];

fn fake() -> Arc<FakeSheets> {
    let f = FakeSheets::default()
        .with_tab(TAB, TAB_ID, applicants())
        .with_tab(TAB_TO, TAB_TO_ID, recipients_tab());
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

fn reg(
    id: &str,
    sheet_id: i64,
    tab: &str,
    notify_tab: &str,
    notify_tab_id: Option<i64>,
) -> SheetReg {
    SheetReg {
        id: id.into(),
        customer_id: "c1".into(),
        spreadsheet_id: SPREAD.into(),
        sheet_id,
        tab_title: tab.into(),
        label: "応募者一覧".into(),
        header_row: 1,
        example_rows: 1,
        customer_can_edit_columns: false,
        disabled: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        notify_tab: notify_tab.into(),
        notify_tab_id,
    }
}

fn setup_store(store: &dyn PortalStore, notify_tab_id: Option<i64>) {
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
        .insert_sheet(reg("s1", TAB_ID, TAB, TAB_TO, notify_tab_id))
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
            tabs_ttl: Duration::ZERO,
            idle_drop: Duration::from_secs(900),
        },
    ))
}

fn notifier_with(
    store: Arc<dyn PortalStore>,
    f: &Arc<FakeSheets>,
    m: &Arc<FakeMailer>,
    cfg_n: NotifyConfig,
) -> Arc<Notifier> {
    Notifier::new(
        Arc::new(PortalAuth::new(store, cfg())),
        m.clone(),
        live(f),
        derive_key(b"test-secret"),
        URL.into(),
        None,
        cfg_n,
    )
}

fn notifier(
    store: Arc<dyn PortalStore>,
    f: &Arc<FakeSheets>,
    m: &Arc<FakeMailer>,
) -> Arc<Notifier> {
    notifier_with(store, f, m, NotifyConfig::default())
}

fn memory(notify_tab_id: Option<i64>) -> Arc<MemoryStore> {
    let s = Arc::new(MemoryStore::default());
    setup_store(s.as_ref(), notify_tab_id);
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

fn rename_tab(f: &FakeSheets, from: &str, to: &str) {
    let mut g = f.tabs.lock().unwrap();
    let t = g.remove(from).unwrap();
    g.insert(to.into(), t);
}

fn to_strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn pending_count(store: &dyn PortalStore) -> usize {
    store
        .notified("s1")
        .iter()
        .filter(|n| n.state == NotifyState::Pending)
        .count()
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
    let store = memory(Some(TAB_TO_ID));
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
    assert_eq!(to, &to_strings(&ALL_TO), "送り先のタブの全員");
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
    let saved = store.notified("s1");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].state, NotifyState::Sent);
    let fp = &saved[0].fingerprint;
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
async fn 再起動を挟んでも同じ行には送らず_未送信の行は送る() {
    let customer = fake();
    let internal = Arc::new(FakeSheets::default());
    let m = Arc::new(FakeMailer::default());

    let s1 = SheetStore::new(internal.clone(), "store-sheet", SheetStoreConfig::default());
    s1.load().await.unwrap();
    setup_store(s1.as_ref(), Some(TAB_TO_ID));
    let n1 = notifier(s1.clone(), &customer, &m);
    n1.tick().await;
    add_applicant(
        &customer,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE, MAIL, "未連絡"],
    );
    n1.tick().await;
    assert_eq!(m.sent().len(), 1);
    // 次の新しい行は送信に失敗し、未送信のまま再起動する
    *m.fail.lock().unwrap() = true;
    add_applicant(
        &customer,
        "2026-10-26T01:30:00.000Z",
        vec!["2026/10/27", "架空 六郎", "090-0000-0106", "", ""],
    );
    n1.tick().await;
    assert_eq!(pending_count(s1.as_ref()), 1, "見つけた行は未送信で残す");
    s1.flush_once().await.unwrap();
    let saved = internal.rows(TAB_NOTIFIED);
    assert_eq!(saved.len(), 3, "見出し・送った行・未送信の行");
    let text = internal.all_text();
    for secret in [NAME, PHONE, "0900000", MAIL, "架空 六郎"] {
        assert!(!text.contains(secret), "社内用シートに {secret} が無い");
    }
    // 送り先のアドレスは操作の記録にだけ残し、「通知済み」には置かない
    let notified_text: String = saved.iter().flatten().cloned().collect();
    assert!(!notified_text.contains('@'));
    *m.fail.lock().unwrap() = false;

    // 再起動: 社内用シートから読み直す。その間に送った行が一時的に消えていた (切り取って貼り直す途中など)
    let s2 = SheetStore::new(internal.clone(), "store-sheet", SheetStoreConfig::default());
    s2.load().await.unwrap();
    assert_eq!(s2.notified("s1").len(), 2, "通知済みの行を読み直した");
    let sent_row = customer.rows(TAB)[4].clone();
    customer
        .tabs
        .lock()
        .unwrap()
        .get_mut(TAB)
        .unwrap()
        .1
        .remove(4);
    touch(&customer, "2026-10-26T02:00:00.000Z");
    let n2 = notifier(s2.clone(), &customer, &m);
    n2.tick().await;
    assert_eq!(m.sent().len(), 2, "再起動の直後でも、未送信の行は送る");
    assert_eq!(pending_count(s2.as_ref()), 0);
    customer
        .tabs
        .lock()
        .unwrap()
        .get_mut(TAB)
        .unwrap()
        .1
        .push(sent_row);
    touch(&customer, "2026-10-26T03:00:00.000Z");
    n2.tick().await;
    assert_eq!(
        m.sent().len(),
        2,
        "前に送った行は、再起動を挟んでも送らない"
    );

    add_applicant(
        &customer,
        "2026-10-26T04:00:00.000Z",
        vec!["2026/10/28", "架空 四郎", "090-0000-0104", "", ""],
    );
    n2.tick().await;
    assert_eq!(m.sent().len(), 3, "本当に新しい行は送る");
}

#[tokio::test]
async fn 送信の許可が無ければ送らず_戻ったらまとめて1通で知らせる() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    *m.not_ready.lock().unwrap() = true;
    let store = memory(Some(TAB_TO_ID));
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
    assert_eq!(
        pending_count(store.as_ref()),
        2,
        "送れなかった行は未送信のまま"
    );

    *m.fail.lock().unwrap() = false;
    n.tick().await;
    assert_eq!(m.sent().len(), 1, "2 行をまとめて 1 回");
    assert_eq!(pending_count(store.as_ref()), 0);
    n.tick().await;
    assert_eq!(m.sent().len(), 1);
}

#[tokio::test]
async fn 一部の宛先に届かなければ_次の確認でその宛先にだけ送る() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    *m.reject.lock().unwrap() = vec!["tenpo@example.com".into()];
    let store = memory(Some(TAB_TO_ID));
    let n = notifier(store.clone(), &f, &m);
    n.tick().await;
    add_applicant(
        &f,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE, MAIL],
    );
    n.tick().await;
    assert_eq!(m.sent()[0].0, to_strings(&ALL_TO));
    let saved = store.notified("s1");
    assert_eq!(
        saved[0].state,
        NotifyState::Pending,
        "全員に届くまで通知済みにしない"
    );
    assert_eq!(saved[0].remaining.len(), 1);
    assert!(
        !saved[0].remaining[0].contains("tenpo"),
        "届いていない宛先はハッシュで持つ"
    );
    assert_eq!(n.status("s1").problem, Some(Problem::PartlyDelivered));

    m.reject.lock().unwrap().clear();
    n.tick().await;
    assert_eq!(
        m.sent()[1].0,
        to_strings(&["tenpo@example.com"]),
        "届かなかった宛先にだけ送る"
    );
    assert_eq!(store.notified("s1")[0].state, NotifyState::Sent);
    n.tick().await;
    assert_eq!(m.sent().len(), 2);
}

#[tokio::test]
async fn 通知の枠を超えたら送らずに保留する() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(Some(TAB_TO_ID));
    let n = notifier_with(
        store.clone(),
        &f,
        &m,
        NotifyConfig {
            sheet_per_hour: 5,
            ..NotifyConfig::default()
        },
    );
    n.tick().await;
    add_applicant(
        &f,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE],
    );
    n.tick().await;
    assert_eq!(m.sent().len(), 1, "3 通");
    add_applicant(
        &f,
        "2026-10-26T02:00:00.000Z",
        vec!["2026/10/27", "架空 七郎", "090-0000-0107"],
    );
    n.tick().await;
    assert_eq!(
        m.sent().len(),
        1,
        "シートの 1 時間の枠 (5 通) を超えるので送らない"
    );
    let st = n.status("s1");
    assert_eq!(st.problem, Some(Problem::RateLimited));
    assert_eq!(st.waiting, 1);
    assert_eq!(pending_count(store.as_ref()), 1);

    // 全体の 1 日の枠も確かめる
    let f2 = fake();
    let m2 = Arc::new(FakeMailer::default());
    let n2 = notifier_with(
        memory(Some(TAB_TO_ID)),
        &f2,
        &m2,
        NotifyConfig {
            total_per_day: 2,
            ..NotifyConfig::default()
        },
    );
    n2.tick().await;
    add_applicant(
        &f2,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE],
    );
    n2.tick().await;
    assert!(m2.sent().is_empty(), "1 日の枠 (2 通) に 3 通は入らない");
    assert_eq!(n2.status("s1").problem, Some(Problem::RateLimited));
}

#[tokio::test]
async fn 送り先は番号で読み_タブの名前を変えても応募者のタブに差し替わらない() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(Some(TAB_TO_ID));
    let n = notifier(store.clone(), &f, &m);
    n.tick().await;
    // 送り先のタブの名前を変え、応募者のタブに元の送り先の名前を付ける
    rename_tab(&f, TAB_TO, "宛先（改名）");
    rename_tab(&f, TAB, TAB_TO);
    add_applicant_on(
        &f,
        TAB_TO,
        "2026-10-26T01:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE, MAIL],
    );
    n.tick().await;
    assert_eq!(m.sent().len(), 1);
    assert_eq!(
        m.sent()[0].0,
        to_strings(&ALL_TO),
        "名前が変わっても同じ番号のタブから読む。応募者のアドレスには送らない"
    );
}

fn add_applicant_on(f: &FakeSheets, tab: &str, at: &str, cells: Vec<&str>) {
    let n = f.rows(tab).len();
    for (i, v) in cells.iter().enumerate() {
        f.set_cell(tab, n, i, v);
    }
    *f.modified.lock().unwrap() = at.into();
}

#[tokio::test]
async fn 送り先が無い_空_応募者のタブなら送らずに印を出す() {
    let f = fake();
    f.tabs
        .lock()
        .unwrap()
        .insert("空のタブ".into(), (10, vec![vec!["送り先".into()]]));
    f.tabs.lock().unwrap().insert(
        "2025年度".into(),
        (
            778,
            vec![
                vec!["応募者名".into(), "メール".into()],
                vec!["架空".into(), "past@applicant.example.com".into()],
            ],
        ),
    );
    let m = Arc::new(FakeMailer::default());
    for (tab_id, want) in [
        (Some(999), Problem::TabMissing(TAB_TO.into())),
        (None, Problem::TabMissing(TAB_TO.into())),
        (Some(10), Problem::NoRecipients),
        (Some(TAB_ID), Problem::ApplicantTab),
        // 送り先を設定していない別の登録 (去年のタブ) も、応募者のタブとして扱う
        (Some(778), Problem::ApplicantTab),
    ] {
        let store = memory(tab_id);
        store
            .insert_sheet(reg("s0-past", 778, "2025年度", "", None))
            .unwrap();
        let n = notifier(store.clone(), &f, &m);
        n.tick().await;
        add_applicant(
            &f,
            &format!("2026-10-26T01:{:02}:00.000Z", f.rows(TAB).len()),
            vec!["2026/10/26", NAME, PHONE, MAIL],
        );
        n.tick().await;
        assert!(m.sent().is_empty(), "{tab_id:?}: 送らない");
        let st = n.status("s1");
        assert_eq!(st.problem, Some(want), "{tab_id:?}");
        assert_eq!(st.waiting, 1, "{tab_id:?}: 直したら送る");
    }
}

#[tokio::test]
async fn 電話とメールの項目が無いシートは通知しない() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(Some(TAB_TO_ID));
    let mut cols = store.columns("s1");
    for c in &mut cols {
        if matches!(c.kind, ColumnKind::Phone | ColumnKind::Email) {
            c.kind = ColumnKind::Text;
        }
    }
    store.replace_columns("s1", cols);
    let n = notifier(store.clone(), &f, &m);
    n.tick().await;
    add_applicant(&f, "2026-10-26T01:00:00.000Z", vec!["2026/10/20", NAME]);
    n.tick().await;
    assert!(m.sent().is_empty());
    assert_eq!(n.status("s1").problem, Some(Problem::NoFingerprintColumns));
    assert_eq!(f.count("batch_get"), 0, "シートも読まない");
}

#[tokio::test]
async fn 指紋に使う項目を変えても一斉には送らない() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(Some(TAB_TO_ID));
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
async fn 止めた顧客とシートには送らず_再開したらその間の行を知らせる() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(Some(TAB_TO_ID));
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

    let mut s = store.sheet("s1").unwrap();
    s.disabled = false;
    store.update_sheet(s).unwrap();
    n.tick().await;
    assert_eq!(
        m.sent().len(),
        1,
        "基準を残していたので、止めている間に入った行も知らせる"
    );
}

#[tokio::test]
async fn 応募者のタブが一時的に読めなくても基準を捨てない() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(Some(TAB_TO_ID));
    let n = notifier(store.clone(), &f, &m);
    n.tick().await;
    let tab = f.tabs.lock().unwrap().remove(TAB).unwrap();
    touch(&f, "2026-10-26T01:00:00.000Z");
    n.tick().await;
    assert_eq!(n.status("s1").problem, Some(Problem::SheetUnreadable));
    f.tabs.lock().unwrap().insert(TAB.into(), tab);
    add_applicant(
        &f,
        "2026-10-26T02:00:00.000Z",
        vec!["2026/10/26", NAME, PHONE, MAIL],
    );
    n.tick().await;
    assert_eq!(m.sent().len(), 1, "戻った後の新しい行は送る");
}

#[tokio::test]
async fn 一年より古い通知済みの行を掃除する() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(Some(TAB_TO_ID));
    store.put_notified(vec![
        NotifiedRow {
            sheet_reg_id: "s1".into(),
            fingerprint: "00aa".into(),
            state: NotifyState::Sent,
            at: Utc::now() - chrono::Duration::days(400),
            remaining: Vec::new(),
        },
        NotifiedRow {
            sheet_reg_id: "s1".into(),
            fingerprint: "00bb".into(),
            state: NotifyState::Sent,
            at: Utc::now() - chrono::Duration::days(30),
            remaining: Vec::new(),
        },
    ]);
    let n = notifier(store.clone(), &f, &m);
    n.tick().await;
    let left: Vec<String> = store
        .notified("s1")
        .into_iter()
        .map(|r| r.fingerprint)
        .collect();
    assert_eq!(left, vec!["00bb".to_string()]);
}

#[tokio::test]
async fn 確認の時間の上限を超えたら残りは次の確認に回す() {
    let f = fake();
    let m = Arc::new(FakeMailer::default());
    let store = memory(Some(TAB_TO_ID));
    let mut s2 = reg("s2", 778, "2025年度", TAB_TO, Some(TAB_TO_ID));
    s2.disabled = true;
    store.insert_sheet(s2).unwrap();
    let n = notifier_with(
        store.clone(),
        &f,
        &m,
        NotifyConfig {
            budget: Duration::ZERO,
            ..NotifyConfig::default()
        },
    );
    n.tick().await;
    assert!(n.status("s1").last_checked_at.is_some());
    assert!(n.status("s2").last_checked_at.is_none(), "上限で残した");
    n.tick().await;
    assert!(
        n.status("s2").last_checked_at.is_some(),
        "次の確認は残したシートから"
    );
}
