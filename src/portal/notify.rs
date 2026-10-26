//! 応募の通知 (計画 14 章、10 章 PR7)。
//!
//! 顧客の応募者管理シートに新しい応募者の行が入ったら、そのシートの「送り先のタブ」に書かれた
//! メールアドレス全員に、宛先ごとに 1 通ずつ知らせる。本文に個人情報は入れない (件名に会社名、本文は画面へのリンクだけ)。
//!
//! - いつ読むか: 送り先のタブを登録したシートだけを [`NotifyConfig::every`] おきに確かめる。
//!   Drive の最終更新時刻 (modifiedTime) が前回と同じなら Sheets を読まない。読み取りは見る画面と同じ
//!   [`LiveSheets`] を使い回す (同じスプレッドシートの確認は 20 秒にまとめる)。
//! - 新しい行の見分け方 (計画 10.2 の「行の指紋」): 種類が電話・メールの項目の値 (電話は数字だけ、メールは小文字) と、
//!   種類が日付の項目のうち並びが最初のもの。見せない項目も使う。同じ指紋の行は上から 1・2 と番号を付ける。
//!   前回の読み取りに無かった指紋の行を新しいとみなす。ただし同じ行番号に前回あった指紋が今どこにも無いなら、
//!   その行の書き換え (電話を後から足した など) とみなして送らない。電話もメールも日付も空の行は、
//!   前回の読み取りでその行番号が空だったときだけ新しいとみなす。
//! - 初めて読むとき (登録の直後・再起動の直後・指紋に使う項目の設定を変えた直後) は、いまある行を基準にするだけで送らない。
//! - 同じ指紋には 1 回だけ送る: 送った行は社内用シートの「通知済み」に鍵付きのハッシュ (HMAC) で残し、再起動しても送らない。
//! - 送れなかった (送り先のタブが無い・空・送信の許可が無い・上限・失敗) 新しい行は、次の確認でまとめて送る。
//!   1 回の確認で新しい行が何件あっても、宛先ごとに 1 通にまとめる。
//! - 保存先 (社内用シート) が読み込み前、または送信元の Gmail の許可が無い間は、シートを読まない。
//!
//! 単一インスタンス前提 (基準はプロセス内に持つ)。ログには件数と理由だけを出し、アドレス・セルの値は出さない。
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use super::auth::PortalAuth;
use super::live::{GridResult, LiveSheets, ReadError};
use super::mail::{Mailer, SendLimits};
use super::store::{ColumnKind, ColumnSetting, LogEntry, LogSource, NotifiedRow, SheetReg};
use super::view::header_positions;

/// 送り先のタブから読む宛先の上限 (違うタブを指定して大勢に送らないように)
pub const MAX_RECIPIENTS: usize = 50;
/// 送り先のタブの名前の長さの上限
pub const MAX_TAB_CHARS: usize = 100;

#[derive(Debug, Clone)]
pub struct NotifyConfig {
    /// シートを確かめる間隔
    pub every: Duration,
    /// 起動してから最初に確かめるまで
    pub first_delay: Duration,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            every: Duration::from_secs(5 * 60),
            first_delay: Duration::from_secs(30),
        }
    }
}

/// 送っていない理由 (管理画面に出す)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    MailNotReady,
    CustomerUnusable,
    SheetStopped,
    SheetUnreadable,
    Busy,
    TabMissing(String),
    ApplicantTab,
    NoRecipients,
    TooManyRecipients,
    RateLimited,
    MailFailed,
}

impl Problem {
    pub fn message(&self) -> String {
        match self {
            Problem::MailNotReady => "メールの送信元の許可がないため、送っていません。「メールの送信元」で許可してください".into(),
            Problem::CustomerUnusable => "顧客を止めている、または段階が「シートだけ」のため、送っていません".into(),
            Problem::SheetStopped => "このシートを止めているため、送っていません".into(),
            Problem::SheetUnreadable => "シートを読めないため、確かめていません。共有が外れていないか確かめてください".into(),
            Problem::Busy => "いまはシートを確かめられませんでした。次の確認でやり直します".into(),
            Problem::TabMissing(t) => format!("送り先のタブ「{t}」が見つかりません。タブの名前を確かめてください"),
            Problem::ApplicantTab => "送り先に応募者のタブが指定されています。別のタブにしてください".into(),
            Problem::NoRecipients => "送り先のタブにメールアドレスがありません".into(),
            Problem::TooManyRecipients => format!("送り先のタブのメールアドレスが多すぎます（{MAX_RECIPIENTS} 件まで）。タブを確かめてください"),
            Problem::RateLimited => "メールを送る回数の上限に当たったため、少し待ってから送ります".into(),
            Problem::MailFailed => "メールを送れませんでした。次の確認でやり直します".into(),
        }
    }
}

/// シートごとの通知の状態 (管理画面に出す)
#[derive(Debug, Clone, Default)]
pub struct NotifyStatus {
    /// 最後に読めた送り先の数 (まだ読んでいなければ None)
    pub recipients: Option<usize>,
    /// 最後に読んだとき、メールアドレスの形でなかったので飛ばした数
    pub skipped: usize,
    /// まだ知らせていない新しい行の数
    pub waiting: usize,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_sent_at: Option<DateTime<Utc>>,
    pub problem: Option<Problem>,
}

/// 1 行の印
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RowMark {
    /// シートの行番号 (1 始まり)
    pub row: u32,
    /// 鍵付きのハッシュ (指紋と同じ指紋の何番目か。指紋が空の行は行番号と中身)
    pub key: String,
    /// 電話・メール・日付のどれかがあった
    pub has_fp: bool,
}

/// 前回の読み取り
#[derive(Debug, Default)]
struct Baseline {
    /// 指紋に使う項目と読む範囲から作る印。変わったら基準を作り直す (送らない)
    tag: String,
    modified: String,
    /// 指紋のある行の鍵
    known: HashSet<String>,
    /// 行番号 → 指紋のある行なら Some(鍵)、指紋の無い行なら None
    rows: HashMap<u32, Option<String>>,
    /// 見つけたがまだ知らせていない行の鍵
    pending: HashSet<String>,
}

pub struct Notifier {
    auth: Arc<PortalAuth>,
    mailer: Arc<dyn Mailer>,
    live: Arc<LiveSheets>,
    limits: Arc<SendLimits>,
    key: [u8; 32],
    portal_url: String,
    store_spreadsheet: Option<String>,
    cfg: NotifyConfig,
    base: Mutex<HashMap<String, Baseline>>,
    status: Mutex<HashMap<String, NotifyStatus>>,
    wake: Notify,
}

// ---- 鍵付きのハッシュ ----

/// HMAC-SHA256 (RFC 2104)
pub(super) fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = Sha256::new()
        .chain_update(ipad)
        .chain_update(msg)
        .finalize();
    Sha256::new()
        .chain_update(opad)
        .chain_update(inner)
        .finalize()
        .into()
}

/// 既存の秘密 (CRM_EMAIL_TOKEN_KEY) から、指紋のハッシュ用の鍵を導く (用途の文字を変えて別の鍵にする)
pub fn derive_key(secret: &[u8]) -> [u8; 32] {
    hmac_sha256(secret, b"hrhr-portal-notify-fingerprint-v1")
}

fn hex16(b: &[u8; 32]) -> String {
    b[..16].iter().map(|x| format!("{x:02x}")).collect()
}

// ---- 行の指紋 ----

fn digits(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '0'..='9' => Some(c),
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32),
            _ => None,
        })
        .collect()
}

/// 指紋に使う項目 (電話・メール・並びが最初の日付)。並びの順
fn fp_columns(cols: &[ColumnSetting]) -> Vec<&ColumnSetting> {
    let mut sorted: Vec<&ColumnSetting> = cols.iter().collect();
    sorted.sort_by(|a, b| (a.order, &a.header).cmp(&(b.order, &b.header)));
    let mut out: Vec<&ColumnSetting> = sorted
        .iter()
        .copied()
        .filter(|c| matches!(c.kind, ColumnKind::Phone | ColumnKind::Email))
        .collect();
    if let Some(d) = sorted.iter().find(|c| c.kind == ColumnKind::Date) {
        out.push(d);
    }
    out
}

/// 基準を作り直すかを決める印 (読む範囲と指紋に使う項目)
fn shape_tag(s: &SheetReg, cols: &[ColumnSetting]) -> String {
    let mut t = format!(
        "{}|{}|{}|{}",
        s.spreadsheet_id, s.sheet_id, s.header_row, s.example_rows
    );
    for c in fp_columns(cols) {
        t.push('|');
        t.push_str(c.kind.as_str());
        t.push(':');
        t.push_str(&c.header);
    }
    hex16(&Sha256::digest(t.as_bytes()).into())
}

/// 読んだ表 (`rows[0]` が見出しの行) の各行の印。記入例の行と、全部空の行は入れない
pub(super) fn row_marks(
    key: &[u8; 32],
    s: &SheetReg,
    cols: &[ColumnSetting],
    rows: &[Vec<String>],
) -> Vec<RowMark> {
    let header_row = s.header_row.max(1);
    let positions = header_positions(rows.first().map(Vec::as_slice).unwrap_or(&[]));
    let fp: Vec<(ColumnKind, Option<usize>)> = fp_columns(cols)
        .into_iter()
        .map(|c| (c.kind, positions.get(&c.header).copied().flatten()))
        .collect();
    let mut seen: HashMap<String, u32> = HashMap::new();
    let mut out = Vec::new();
    for (i, r) in rows.iter().enumerate().skip(1 + s.example_rows as usize) {
        if r.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let row = header_row + i as u32;
        let parts: Vec<String> = fp
            .iter()
            .map(|(kind, at)| {
                let v = at.and_then(|i| r.get(i)).map(String::as_str).unwrap_or("");
                match kind {
                    ColumnKind::Phone => digits(v),
                    ColumnKind::Email => v.trim().to_lowercase(),
                    _ => v.trim().to_string(),
                }
            })
            .collect();
        let has_fp = parts.iter().any(|p| !p.is_empty());
        let msg = if has_fp {
            let base = parts.join("\u{1f}");
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            format!("v1\u{1f}{}\u{1f}{base}\u{1f}{n}", s.id)
        } else {
            format!("v1e\u{1f}{}\u{1f}{row}\u{1f}{}", s.id, r.join("\u{1f}"))
        };
        out.push(RowMark {
            row,
            key: hex16(&hmac_sha256(key, msg.as_bytes())),
            has_fp,
        });
    }
    out
}

/// 前回の基準と比べて新しい行 (計画 10.2)
fn new_rows<'a>(prev: &Baseline, cur: &'a [RowMark]) -> Vec<&'a RowMark> {
    let present: HashSet<&str> = cur
        .iter()
        .filter(|m| m.has_fp)
        .map(|m| m.key.as_str())
        .collect();
    cur.iter()
        .filter(|m| {
            if m.has_fp {
                if prev.known.contains(&m.key) {
                    return false;
                }
                // 同じ行番号に前回あった指紋が今どこにも無い: その行の書き換え
                !matches!(prev.rows.get(&m.row), Some(Some(old)) if old != &m.key && !present.contains(old.as_str()))
            } else {
                !prev.rows.contains_key(&m.row)
            }
        })
        .collect()
}

// ---- 送り先 ----

fn is_sep(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ',' | ';'
                | '、'
                | '，'
                | '；'
                | '<'
                | '>'
                | '('
                | ')'
                | '（'
                | '）'
                | '"'
                | '「'
                | '」'
                | '['
                | ']'
                | ':'
                | '：'
                | '・'
        )
}

/// タブの全部のマスから、メールアドレスの形のものを拾う (列の並びに頼らない)。
/// 重なりは 1 回、`@` があるのに形の正しくないものは飛ばして数える
pub(super) fn extract_addresses(cells: &[Vec<String>]) -> (Vec<String>, usize) {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut skipped = 0;
    for token in cells.iter().flatten().flat_map(|c| c.split(is_sep)) {
        let t = token.trim().trim_matches('.');
        if !(t.contains('@') || t.contains('＠')) {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if crate::crm::email::valid_address(&lower) {
            if seen.insert(lower.clone()) {
                out.push(lower);
            }
        } else {
            skipped += 1;
        }
    }
    (out, skipped)
}

/// 件名と本文。個人情報は入れない (会社名と画面へのリンクだけ)
pub fn notify_message(customer_name: &str, portal_url: &str) -> (String, String) {
    let name: String = customer_name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let name = name.trim();
    let subject = if name.is_empty() {
        "新しい応募があります".to_string()
    } else {
        format!("新しい応募があります（{name}）")
    };
    let body = format!(
        "応募者の管理に、新しい応募があります。\n\
内容は次のページにログインしてご確認ください。\n\n{portal_url}\n\n\
お心当たりがない場合は、弊社の担当者にご連絡ください。\n"
    );
    (subject, body)
}

impl Notifier {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        auth: Arc<PortalAuth>,
        mailer: Arc<dyn Mailer>,
        live: Arc<LiveSheets>,
        limits: Arc<SendLimits>,
        key: [u8; 32],
        portal_url: String,
        store_spreadsheet: Option<String>,
        cfg: NotifyConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            auth,
            mailer,
            live,
            limits,
            key,
            portal_url,
            store_spreadsheet,
            cfg,
            base: Mutex::new(HashMap::new()),
            status: Mutex::new(HashMap::new()),
            wake: Notify::new(),
        })
    }

    /// 裏で確かめ続ける (今の tokio の上で)
    pub fn spawn(self: &Arc<Self>) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(me.cfg.first_delay).await;
            loop {
                // 何かで止まっても (panic) 次の確認は続ける
                if let Err(e) = tokio::spawn({
                    let me = me.clone();
                    async move { me.tick().await }
                })
                .await
                {
                    if e.is_panic() {
                        tracing::error!(
                            "[portal notify] 応募の確認の処理が止まった。次の確認でやり直す"
                        );
                    }
                }
                tokio::select! {
                    _ = tokio::time::sleep(me.cfg.every) => {}
                    _ = me.wake.notified() => {}
                }
            }
        });
    }

    /// 送り先のタブを変えた: 読めた送り先を忘れ、次の確認をすぐ始める
    pub fn tab_changed(&self, sheet_reg_id: &str) {
        self.set_status(sheet_reg_id, |st| {
            st.recipients = None;
            st.skipped = 0;
            st.problem = None;
        });
        self.wake.notify_one();
    }

    /// 管理画面に出す状態
    pub fn status(&self, sheet_reg_id: &str) -> NotifyStatus {
        let mut st = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(sheet_reg_id)
            .cloned()
            .unwrap_or_default();
        // 再起動のあとも、最後に送った時刻は社内用シートの「通知済み」から出す
        let saved = self
            .auth
            .store()
            .notified(sheet_reg_id)
            .iter()
            .map(|n| n.sent_at)
            .max();
        st.last_sent_at = st.last_sent_at.max(saved);
        st
    }

    fn set_status(&self, id: &str, f: impl FnOnce(&mut NotifyStatus)) {
        let mut g = self.status.lock().unwrap_or_else(|e| e.into_inner());
        f(g.entry(id.to_string()).or_default());
    }

    fn take_base(&self, id: &str) -> Option<Baseline> {
        self.base
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
    }

    fn put_base(&self, id: &str, b: Baseline) {
        self.base
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), b);
    }

    /// 送り先のタブを登録したシートを 1 回ずつ確かめる
    pub async fn tick(&self) {
        let store = self.auth.store();
        if !store.ready() {
            return;
        }
        let sheets: Vec<SheetReg> = store
            .sheets(None)
            .into_iter()
            .filter(|s| !s.notify_tab.trim().is_empty())
            .collect();
        {
            let ids: HashSet<&str> = sheets.iter().map(|s| s.id.as_str()).collect();
            self.base
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|k, _| ids.contains(k.as_str()));
            self.status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|k, _| ids.contains(k.as_str()));
        }
        let mail_ok = self.mailer.ready();
        for s in &sheets {
            self.check_sheet(s, &sheets, mail_ok).await;
        }
    }

    async fn check_sheet(&self, s: &SheetReg, all: &[SheetReg], mail_ok: bool) {
        let now = Utc::now();
        self.set_status(&s.id, |st| st.last_checked_at = Some(now));
        let problem = |p: Problem| {
            self.set_status(&s.id, |st| st.problem = Some(p));
        };
        let store = self.auth.store();
        if s.disabled || self.store_spreadsheet.as_deref() == Some(s.spreadsheet_id.as_str()) {
            self.take_base(&s.id);
            return problem(Problem::SheetStopped);
        }
        if !store.customer(&s.customer_id).is_some_and(|c| c.usable()) {
            return problem(Problem::CustomerUnusable);
        }
        if !mail_ok {
            return problem(Problem::MailNotReady);
        }
        let modified = match self.live.modified(&s.spreadsheet_id).await {
            Ok(m) => m,
            Err(e) => {
                return match ReadError::from(e) {
                    ReadError::NotShared => {
                        self.take_base(&s.id);
                        problem(Problem::SheetUnreadable)
                    }
                    _ => problem(Problem::Busy),
                };
            }
        };
        let cols = store.columns(&s.id);
        let tag = shape_tag(s, &cols);
        let prev = self.take_base(&s.id);
        // 送り先をまだ読めていない (登録の直後・送り先のタブを変えた直後・タブが無かった)
        let need_recipients = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&s.id)
            .is_none_or(|st| st.recipients.is_none_or(|n| n == 0 || n > MAX_RECIPIENTS));
        let unchanged = prev.as_ref().is_some_and(|b| {
            b.tag == tag && b.modified == modified && b.pending.is_empty() && !need_recipients
        });
        if unchanged {
            // 変わっていない: Sheets を読まない
            if let Some(b) = prev {
                self.put_base(&s.id, b);
            }
            self.set_status(&s.id, |st| st.problem = None);
            return;
        }
        let grid = match self.live.grid(s, Some(&modified)).await {
            Ok(GridResult::Fresh(g)) => g,
            Ok(GridResult::Stale(_)) => {
                if let Some(b) = prev {
                    self.put_base(&s.id, b);
                }
                return problem(Problem::Busy);
            }
            Err(e) => {
                if matches!(e, ReadError::NotShared | ReadError::TabNotFound) {
                    return problem(Problem::SheetUnreadable);
                }
                if let Some(b) = prev {
                    self.put_base(&s.id, b);
                }
                return problem(Problem::Busy);
            }
        };
        let marks = row_marks(&self.key, s, &cols, &grid.rows);
        let mut next = Baseline {
            tag: tag.clone(),
            modified: modified.clone(),
            known: marks
                .iter()
                .filter(|m| m.has_fp)
                .map(|m| m.key.clone())
                .collect(),
            rows: marks
                .iter()
                .map(|m| (m.row, m.has_fp.then(|| m.key.clone())))
                .collect(),
            pending: HashSet::new(),
        };
        let prev = match prev.filter(|b| b.tag == tag) {
            Some(b) => b,
            None => {
                // 初めて読んだ (登録・再起動の直後、指紋に使う項目を変えた直後): 基準にするだけで送らない
                tracing::info!(
                    "[portal notify] 応募の通知の基準を作った（{} 行）",
                    marks.len()
                );
                self.put_base(&s.id, next);
                self.set_status(&s.id, |st| {
                    st.waiting = 0;
                    st.problem = None;
                });
                self.refresh_recipients(s, all).await;
                return;
            }
        };
        let sent: HashSet<String> = store
            .notified(&s.id)
            .into_iter()
            .map(|n| n.fingerprint)
            .collect();
        let mut fresh: Vec<&RowMark> = new_rows(&prev, &marks);
        for m in &marks {
            if prev.pending.contains(&m.key) && !fresh.iter().any(|f| f.key == m.key) {
                fresh.push(m);
            }
        }
        fresh.retain(|m| !sent.contains(&m.key));
        fresh.sort_by_key(|m| m.row);
        if fresh.is_empty() {
            self.put_base(&s.id, next);
            self.set_status(&s.id, |st| {
                st.waiting = 0;
                st.problem = None;
            });
            if need_recipients {
                self.refresh_recipients(s, all).await;
            }
            return;
        }
        // 送る直前に送り先を読む
        let result = self.send(s, all, &fresh).await;
        match result {
            Ok(()) => {
                self.set_status(&s.id, |st| {
                    st.waiting = 0;
                    st.problem = None;
                    st.last_sent_at = Some(Utc::now());
                });
            }
            Err(p) => {
                next.pending = fresh.iter().map(|m| m.key.clone()).collect();
                let n = next.pending.len();
                tracing::warn!(
                    "[portal notify] 新しい応募 {n} 件を知らせられなかった: {}",
                    p.message_for_log()
                );
                self.set_status(&s.id, |st| {
                    st.waiting = n;
                    st.problem = Some(p);
                });
            }
        }
        self.put_base(&s.id, next);
    }

    /// 送り先のタブの番号では読まない応募者のタブ (同じスプレッドシートに登録したもの)
    fn forbidden_tabs(s: &SheetReg, all: &[SheetReg]) -> Vec<i64> {
        let mut v: Vec<i64> = all
            .iter()
            .filter(|o| o.spreadsheet_id == s.spreadsheet_id)
            .map(|o| o.sheet_id)
            .collect();
        v.push(s.sheet_id);
        v
    }

    /// 送り先を読み、状態に残す
    async fn recipients(&self, s: &SheetReg, all: &[SheetReg]) -> Result<Vec<String>, Problem> {
        let tab = s.notify_tab.trim();
        if all
            .iter()
            .filter(|o| o.spreadsheet_id == s.spreadsheet_id)
            .any(|o| o.tab_title.trim() == tab)
        {
            return Err(Problem::ApplicantTab);
        }
        let cells = self
            .live
            .tab_cells(&s.spreadsheet_id, tab, &Self::forbidden_tabs(s, all))
            .await
            .map_err(|e| match e {
                ReadError::TabNotFound => Problem::TabMissing(tab.to_string()),
                ReadError::NotShared => Problem::SheetUnreadable,
                ReadError::Google(_) => Problem::Busy,
            })?;
        let (addrs, skipped) = extract_addresses(&cells);
        self.set_status(&s.id, |st| {
            st.recipients = Some(addrs.len());
            st.skipped = skipped;
        });
        if skipped > 0 {
            tracing::info!("[portal notify] 送り先のタブで、メールアドレスの形でないものを {skipped} 件飛ばした");
        }
        if addrs.is_empty() {
            return Err(Problem::NoRecipients);
        }
        if addrs.len() > MAX_RECIPIENTS {
            return Err(Problem::TooManyRecipients);
        }
        Ok(addrs)
    }

    async fn refresh_recipients(&self, s: &SheetReg, all: &[SheetReg]) {
        match self.recipients(s, all).await {
            Ok(_) => {}
            Err(p) => {
                if matches!(p, Problem::TabMissing(_) | Problem::ApplicantTab) {
                    self.set_status(&s.id, |st| {
                        st.recipients = None;
                        st.skipped = 0;
                    });
                }
                self.set_status(&s.id, |st| st.problem = Some(p));
            }
        }
    }

    async fn send(&self, s: &SheetReg, all: &[SheetReg], rows: &[&RowMark]) -> Result<(), Problem> {
        let to = match self.recipients(s, all).await {
            Ok(v) => v,
            Err(p) => {
                if matches!(p, Problem::TabMissing(_) | Problem::ApplicantTab) {
                    self.set_status(&s.id, |st| {
                        st.recipients = None;
                        st.skipped = 0;
                    });
                }
                return Err(p);
            }
        };
        let store = self.auth.store();
        let name = store
            .customer(&s.customer_id)
            .map(|c| c.name)
            .unwrap_or_default();
        let key = format!("notify:{}", s.id);
        if !self.limits.take(&key, to.len(), std::time::Instant::now()) {
            return Err(Problem::RateLimited);
        }
        let (subject, body) = notify_message(&name, &self.portal_url);
        let delivered = self
            .mailer
            .send(&to, &subject, &body)
            .await
            .map_err(|_| Problem::MailFailed)?;
        let now = Utc::now();
        store.insert_notified(
            rows.iter()
                .map(|m| NotifiedRow {
                    sheet_reg_id: s.id.clone(),
                    fingerprint: m.key.clone(),
                    sent_at: now,
                })
                .collect(),
        );
        let row_list = rows
            .iter()
            .map(|m| m.row.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut e = LogEntry::new(now, "system", LogSource::System, "notify");
        e.customer_id = s.customer_id.clone();
        e.sheet_reg_id = s.id.clone();
        e.row = row_list;
        e.detail = format!(
            "新しい応募の通知を送った（新しい行 {} 件・{} 通{}）。送り先: {}",
            rows.len(),
            delivered.len(),
            if delivered.len() < to.len() {
                format!("。{} 通は送れなかった", to.len() - delivered.len())
            } else {
                String::new()
            },
            delivered.join(", ")
        );
        store.append_log(e);
        tracing::info!(
            "[portal notify] 新しい応募の通知を送った（新しい行 {} 件・{} 通）",
            rows.len(),
            delivered.len()
        );
        Ok(())
    }
}

impl Problem {
    /// ログ用 (タブの名前も出さない)
    fn message_for_log(&self) -> &'static str {
        match self {
            Problem::MailNotReady => "送信元の許可がない",
            Problem::CustomerUnusable => "顧客が使えない状態",
            Problem::SheetStopped => "シートを止めている",
            Problem::SheetUnreadable => "シートを読めない",
            Problem::Busy => "シートを確かめられない",
            Problem::TabMissing(_) => "送り先のタブが無い",
            Problem::ApplicantTab => "送り先が応募者のタブ",
            Problem::NoRecipients => "送り先が空",
            Problem::TooManyRecipients => "送り先が多すぎる",
            Problem::RateLimited => "送る回数の上限",
            Problem::MailFailed => "送信の失敗",
        }
    }
}
