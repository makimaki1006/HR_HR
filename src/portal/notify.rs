//! 応募の通知 (計画 14 章、10 章 PR7)。
//!
//! 顧客の応募者管理シートに新しい応募者の行が入ったら、そのシートの「送り先のタブ」に書かれた
//! メールアドレス全員に、宛先ごとに 1 通ずつ知らせる。本文に個人情報は入れない (件名に会社名、本文は画面へのリンクだけ)。
//!
//! - いつ読むか: 送り先のタブを登録したシートだけを [`NotifyConfig::every`] おきに確かめる。
//!   Drive の最終更新時刻 (modifiedTime) が前回と同じで、未送信の行も無ければ Sheets を読まない。
//!   読み取りは見る画面と同じ [`LiveSheets`] を使い回す。1 回の確認にかける時間には上限
//!   ([`NotifyConfig::budget`]) があり、超えたら残りのシートは次の確認に回す (次はそこから始める)。
//! - 送り先のタブは番号 (sheetId) で読む。名前は表示用。同じスプレッドシートに登録した応募者のタブ
//!   (送り先を設定していない登録も含む) は、送り先として読まない。
//! - 新しい行の見分け方 (計画 10.2 の「行の指紋」): 種類が電話・メールの項目の値 (電話は数字だけ、メールは小文字) と、
//!   種類が日付の項目のうち並びが最初のもの。見せない項目も使う。同じ指紋の行は上から 1・2 と番号を付ける。
//!   電話・メールの種類の項目がシートに無いときは、通知しない (日付だけでは別の人を同じ人とみなしてしまう)。
//!   前回の読み取りに無かった指紋の行を新しいとみなす。ただし同じ行番号に前回あった指紋が今どこにも無いなら、
//!   その行の書き換え (電話を後から足した など) とみなして送らない。電話もメールも日付も空の行は、
//!   前回の読み取りでその行番号が空だったときだけ新しいとみなす。
//! - 初めて読むとき (登録の直後・再起動の直後・指紋に使う項目の設定を変えた直後) は、いまある行を基準にするだけで送らない。
//! - 見つけた新しい行は、すぐ社内用シートの「通知済み」に「未送信」で残す (再起動しても送る)。
//!   送り先の全員に届いたら「送った」にする。一部の宛先に届かなければ、その宛先 (鍵付きのハッシュ) を残し、
//!   次の確認でその宛先にだけ送る。1 回の確認で新しい行が何件あっても、宛先ごとに 1 通にまとめる。
//! - 送る回数は確認コードとは別の枠: 全体で 1 時間 100 通・1 日 500 通、シートごとに 1 時間 50 通。
//!   当たったら送らずに未送信のまま残す。
//! - 保存先 (社内用シート) が読み込み前、または送信元の Gmail の許可が無い間は、シートを読まない。
//!
//! 単一インスタンス前提 (基準はプロセス内に持つ)。ログには件数と理由だけを出し、アドレス・セルの値は出さない。
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use super::auth::PortalAuth;
use super::live::{GridResult, LiveSheets, ReadError};
use super::mail::Mailer;
use super::store::{
    ColumnKind, ColumnSetting, LogEntry, LogSource, NotifiedRow, NotifyState, SheetReg,
};
use super::view::header_positions;

/// 送り先のタブから読む宛先の上限 (違うタブを指定して大勢に送らないように)
pub const MAX_RECIPIENTS: usize = 50;
/// 送り先のタブの名前の長さの上限
pub const MAX_TAB_CHARS: usize = 100;
/// 「通知済み」の行を残す期間
const KEEP_NOTIFIED: chrono::Duration = chrono::Duration::days(365);
/// 「通知済み」の掃除の間隔
const PURGE_EVERY: Duration = Duration::from_secs(24 * 3600);

#[derive(Debug, Clone)]
pub struct NotifyConfig {
    /// シートを確かめる間隔
    pub every: Duration,
    /// 起動してから最初に確かめるまで
    pub first_delay: Duration,
    /// 1 回の確認 (全シート) にかける時間の上限。超えたら残りは次の確認に回す
    pub budget: Duration,
    /// 全体で 1 時間に送る通数の上限 (確認コードとは別の枠)
    pub total_per_hour: usize,
    /// 全体で 1 日に送る通数の上限
    pub total_per_day: usize,
    /// シートごとに 1 時間に送る通数の上限
    pub sheet_per_hour: usize,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            every: Duration::from_secs(5 * 60),
            first_delay: Duration::from_secs(30),
            budget: Duration::from_secs(120),
            total_per_hour: 100,
            total_per_day: 500,
            sheet_per_hour: 50,
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
    NoFingerprintColumns,
    TabMissing(String),
    ApplicantTab,
    NoRecipients,
    TooManyRecipients,
    RateLimited,
    MailFailed,
    PartlyDelivered,
}

impl Problem {
    pub fn message(&self) -> String {
        match self {
            Problem::MailNotReady => "メールの送信元の許可がないため、送っていません。「メールの送信元」で許可してください".into(),
            Problem::CustomerUnusable => "顧客を止めている、または段階が「シートだけ」のため、送っていません".into(),
            Problem::SheetStopped => "このシートを止めているため、送っていません".into(),
            Problem::SheetUnreadable => "シートを読めないため、確かめていません。共有が外れていないか確かめてください".into(),
            Problem::Busy => "いまはシートを確かめられませんでした。次の確認でやり直します".into(),
            Problem::NoFingerprintColumns => "新しい応募を見分けるには、電話番号かメールアドレスの項目が要ります。「項目の設定」で、その列の種類を「電話番号」か「メールアドレス」にしてください".into(),
            Problem::TabMissing(t) => format!("送り先のタブ「{t}」が見つかりません。消された可能性があります。送り先のタブを設定し直してください"),
            Problem::ApplicantTab => "送り先に応募者のタブが指定されています。別のタブにしてください".into(),
            Problem::NoRecipients => "送り先のタブにメールアドレスがありません".into(),
            Problem::TooManyRecipients => format!("送り先のタブのメールアドレスが多すぎます（{MAX_RECIPIENTS} 件まで）。タブを確かめてください"),
            Problem::RateLimited => "送る回数の上限（全体で 1 時間 100 通・1 日 500 通、このシートで 1 時間 50 通）に当たったため、保留しています。上限が空いたら送ります".into(),
            Problem::MailFailed => "メールを送れませんでした。次の確認でやり直します".into(),
            Problem::PartlyDelivered => "一部の宛先に届きませんでした。次の確認で、その宛先にだけ送ります".into(),
        }
    }

    /// ログ用 (タブの名前も出さない)
    fn for_log(&self) -> &'static str {
        match self {
            Problem::MailNotReady => "送信元の許可がない",
            Problem::CustomerUnusable => "顧客が使えない状態",
            Problem::SheetStopped => "シートを止めている",
            Problem::SheetUnreadable => "シートを読めない",
            Problem::Busy => "シートを確かめられない",
            Problem::NoFingerprintColumns => "電話・メールの項目が無い",
            Problem::TabMissing(_) => "送り先のタブが無い",
            Problem::ApplicantTab => "送り先が応募者のタブ",
            Problem::NoRecipients => "送り先が空",
            Problem::TooManyRecipients => "送り先が多すぎる",
            Problem::RateLimited => "送る回数の上限",
            Problem::MailFailed => "送信の失敗",
            Problem::PartlyDelivered => "一部の宛先に届かなかった",
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
    /// まだ知らせ終わっていない新しい行の数
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
}

/// 通知の送る回数 (確認コードとは別の枠。プロセス内で数える)
#[derive(Default)]
struct NotifyLimits {
    total: VecDeque<Instant>,
    sheets: HashMap<String, VecDeque<Instant>>,
}

impl NotifyLimits {
    fn take(&mut self, cfg: &NotifyConfig, sheet: &str, mails: usize, now: Instant) -> bool {
        let hour = Duration::from_secs(3600);
        let day = Duration::from_secs(24 * 3600);
        while self
            .total
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= day)
        {
            self.total.pop_front();
        }
        self.sheets.retain(|_, q| {
            while q
                .front()
                .is_some_and(|t| now.saturating_duration_since(*t) >= hour)
            {
                q.pop_front();
            }
            !q.is_empty()
        });
        let in_hour = self
            .total
            .iter()
            .filter(|t| now.saturating_duration_since(**t) < hour)
            .count();
        let sheet_hour = self.sheets.get(sheet).map_or(0, VecDeque::len);
        if in_hour + mails > cfg.total_per_hour
            || self.total.len() + mails > cfg.total_per_day
            || sheet_hour + mails > cfg.sheet_per_hour
        {
            return false;
        }
        self.total.extend(std::iter::repeat_n(now, mails));
        self.sheets
            .entry(sheet.to_string())
            .or_default()
            .extend(std::iter::repeat_n(now, mails));
        true
    }
}

pub struct Notifier {
    auth: Arc<PortalAuth>,
    mailer: Arc<dyn Mailer>,
    live: Arc<LiveSheets>,
    key: [u8; 32],
    portal_url: String,
    store_spreadsheet: Option<String>,
    cfg: NotifyConfig,
    base: Mutex<HashMap<String, Baseline>>,
    status: Mutex<HashMap<String, NotifyStatus>>,
    limits: Mutex<NotifyLimits>,
    /// 次の確認で最初に見るシート (時間の上限で残したシートから始める)
    cursor: Mutex<Option<String>>,
    last_purge: Mutex<Option<Instant>>,
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

/// 指紋に使える項目 (種類が電話かメール) が項目の設定にあるか
pub fn has_fp_columns(cols: &[ColumnSetting]) -> bool {
    cols.iter()
        .any(|c| matches!(c.kind, ColumnKind::Phone | ColumnKind::Email))
}

/// 種類が電話かメールの項目の見出しが、読んだ表の見出しに 1 つでも (重ならずに) あるか
fn fp_headers_present(cols: &[ColumnSetting], header: &[String]) -> bool {
    let positions = header_positions(header);
    cols.iter()
        .filter(|c| matches!(c.kind, ColumnKind::Phone | ColumnKind::Email))
        .any(|c| positions.get(&c.header).copied().flatten().is_some())
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

/// 同じスプレッドシートに登録した応募者のタブの番号 (送り先を設定していない登録も含む)
fn applicant_tabs(s: &SheetReg, all: &[SheetReg]) -> Vec<i64> {
    let mut v: Vec<i64> = all
        .iter()
        .filter(|o| o.spreadsheet_id == s.spreadsheet_id)
        .map(|o| o.sheet_id)
        .collect();
    v.push(s.sheet_id);
    v
}

impl Notifier {
    pub fn new(
        auth: Arc<PortalAuth>,
        mailer: Arc<dyn Mailer>,
        live: Arc<LiveSheets>,
        key: [u8; 32],
        portal_url: String,
        store_spreadsheet: Option<String>,
        cfg: NotifyConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            auth,
            mailer,
            live,
            key,
            portal_url,
            store_spreadsheet,
            cfg,
            base: Mutex::new(HashMap::new()),
            status: Mutex::new(HashMap::new()),
            limits: Mutex::new(NotifyLimits::default()),
            cursor: Mutex::new(None),
            last_purge: Mutex::new(None),
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
            .filter(|n| n.state == NotifyState::Sent)
            .map(|n| n.at)
            .max();
        st.last_sent_at = st.last_sent_at.max(saved);
        st
    }

    fn set_status(&self, id: &str, f: impl FnOnce(&mut NotifyStatus)) {
        let mut g = self.status.lock().unwrap_or_else(|e| e.into_inner());
        f(g.entry(id.to_string()).or_default());
    }

    fn problem(&self, id: &str, p: Problem) {
        self.set_status(id, |st| st.problem = Some(p));
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

    fn purge_if_due(&self) {
        let due = {
            let mut g = self.last_purge.lock().unwrap_or_else(|e| e.into_inner());
            let due = g.is_none_or(|t| t.elapsed() >= PURGE_EVERY);
            if due {
                *g = Some(Instant::now());
            }
            due
        };
        if due {
            let n = self.auth.store().purge_notified(Utc::now() - KEEP_NOTIFIED);
            if n > 0 {
                tracing::info!("[portal notify] 1 年より古い「通知済み」の行を {n} 行消した");
            }
        }
    }

    /// 送り先のタブを登録したシートを 1 回ずつ確かめる。時間の上限を超えたら残りは次の確認に回す
    pub async fn tick(&self) {
        let store = self.auth.store();
        if !store.ready() {
            return;
        }
        self.purge_if_due();
        let all = store.sheets(None);
        let mut sheets: Vec<SheetReg> = all
            .iter()
            .filter(|s| !s.notify_tab.trim().is_empty())
            .cloned()
            .collect();
        sheets.sort_by(|a, b| a.id.cmp(&b.id));
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
        // 前の確認で残したシートから始める
        let start = self
            .cursor
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .and_then(|c| sheets.iter().position(|s| s.id >= c))
            .unwrap_or(0);
        sheets.rotate_left(start);
        let mail_ok = self.mailer.ready();
        let began = Instant::now();
        for (i, s) in sheets.iter().enumerate() {
            if i > 0 && began.elapsed() >= self.cfg.budget {
                tracing::warn!(
                    "[portal notify] 確認の時間の上限に当たった。残り {} シートは次の確認に回す",
                    sheets.len() - i
                );
                *self.cursor.lock().unwrap_or_else(|e| e.into_inner()) = Some(s.id.clone());
                break;
            }
            self.check_sheet(s, &all, mail_ok).await;
        }
    }

    async fn check_sheet(&self, s: &SheetReg, all: &[SheetReg], mail_ok: bool) {
        let now = Utc::now();
        self.set_status(&s.id, |st| st.last_checked_at = Some(now));
        let store = self.auth.store();
        // 止めた・使えない間は読まない。基準は残す (再開したら、その間に入った行も知らせる)
        if s.disabled || self.store_spreadsheet.as_deref() == Some(s.spreadsheet_id.as_str()) {
            return self.problem(&s.id, Problem::SheetStopped);
        }
        if !store.customer(&s.customer_id).is_some_and(|c| c.usable()) {
            return self.problem(&s.id, Problem::CustomerUnusable);
        }
        if !mail_ok {
            return self.problem(&s.id, Problem::MailNotReady);
        }
        let cols = store.columns(&s.id);
        if !has_fp_columns(&cols) {
            return self.problem(&s.id, Problem::NoFingerprintColumns);
        }
        let modified = match self.live.modified(&s.spreadsheet_id).await {
            Ok(m) => m,
            Err(e) => {
                return self.problem(
                    &s.id,
                    match ReadError::from(e) {
                        ReadError::NotShared => Problem::SheetUnreadable,
                        _ => Problem::Busy,
                    },
                );
            }
        };
        let tag = shape_tag(s, &cols);
        let prev = self.take_base(&s.id);
        let has_pending = store
            .notified(&s.id)
            .iter()
            .any(|n| n.state == NotifyState::Pending);
        // 送り先をまだ読めていない (登録の直後・送り先のタブを変えた直後・タブが無い・空)
        let need_recipients = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&s.id)
            .is_none_or(|st| st.recipients.is_none_or(|n| n == 0 || n > MAX_RECIPIENTS));
        let unchanged = prev
            .as_ref()
            .is_some_and(|b| b.tag == tag && b.modified == modified)
            && !has_pending
            && !need_recipients;
        if unchanged {
            // 変わっていない: Sheets を読まない
            if let Some(b) = prev {
                self.put_base(&s.id, b);
            }
            self.set_status(&s.id, |st| {
                st.problem = None;
                st.waiting = 0;
            });
            return;
        }
        let restore = |prev: Option<Baseline>, p: Problem| {
            if let Some(b) = prev {
                self.put_base(&s.id, b);
            }
            self.problem(&s.id, p);
        };
        let grid = match self.live.grid(s, Some(&modified)).await {
            Ok(GridResult::Fresh(g)) => g,
            Ok(GridResult::Stale(_)) => return restore(prev, Problem::Busy),
            Err(ReadError::NotShared | ReadError::TabNotFound) => {
                return restore(prev, Problem::SheetUnreadable)
            }
            Err(ReadError::Google(_)) => return restore(prev, Problem::Busy),
        };
        let header: &[String] = grid.rows.first().map(Vec::as_slice).unwrap_or(&[]);
        if !fp_headers_present(&cols, header) {
            return restore(prev, Problem::NoFingerprintColumns);
        }
        let marks = row_marks(&self.key, s, &cols, &grid.rows);
        let next = Baseline {
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
        };
        // 新しい行は、すぐ「未送信」で残す (再起動しても送る)
        match prev.filter(|b| b.tag == tag) {
            Some(prev) => {
                let saved: HashSet<String> = store
                    .notified(&s.id)
                    .into_iter()
                    .map(|n| n.fingerprint)
                    .collect();
                let fresh: Vec<NotifiedRow> = new_rows(&prev, &marks)
                    .into_iter()
                    .filter(|m| !saved.contains(&m.key))
                    .map(|m| NotifiedRow {
                        sheet_reg_id: s.id.clone(),
                        fingerprint: m.key.clone(),
                        state: NotifyState::Pending,
                        at: now,
                        remaining: Vec::new(),
                    })
                    .collect();
                if !fresh.is_empty() {
                    tracing::info!("[portal notify] 新しい応募を {} 件見つけた", fresh.len());
                    store.put_notified(fresh);
                }
            }
            None => {
                // 初めて読んだ (登録・再起動の直後、指紋に使う項目を変えた直後): 基準にするだけで送らない
                tracing::info!(
                    "[portal notify] 応募の通知の基準を作った（{} 行）",
                    marks.len()
                );
            }
        }
        self.put_base(&s.id, next);
        let row_of: HashMap<&str, u32> = marks.iter().map(|m| (m.key.as_str(), m.row)).collect();
        // シートに今ある未送信の行 (消えた行には送らない)
        let pending: Vec<NotifiedRow> = store
            .notified(&s.id)
            .into_iter()
            .filter(|n| {
                n.state == NotifyState::Pending && row_of.contains_key(n.fingerprint.as_str())
            })
            .collect();
        if pending.is_empty() {
            self.set_status(&s.id, |st| {
                st.waiting = 0;
                st.problem = None;
            });
            if need_recipients {
                if let Err(p) = self.recipients(s, all).await {
                    self.problem(&s.id, p);
                }
            }
            return;
        }
        let n = pending.len();
        match self.deliver(s, all, &pending, &row_of).await {
            Ok(left) => self.set_status(&s.id, |st| {
                st.waiting = left;
                st.problem = (left > 0).then_some(Problem::PartlyDelivered);
            }),
            Err(p) => {
                tracing::warn!(
                    "[portal notify] 新しい応募 {n} 件を知らせられなかった: {}",
                    p.for_log()
                );
                self.set_status(&s.id, |st| {
                    st.waiting = n;
                    st.problem = Some(p);
                });
            }
        }
    }

    /// 送り先を読み、状態に残す
    async fn recipients(&self, s: &SheetReg, all: &[SheetReg]) -> Result<Vec<String>, Problem> {
        let forbidden = applicant_tabs(s, all);
        let missing = || Problem::TabMissing(s.notify_tab.trim().to_string());
        let clear = || {
            self.set_status(&s.id, |st| {
                st.recipients = None;
                st.skipped = 0;
            })
        };
        let Some(tab_id) = s.notify_tab_id else {
            clear();
            return Err(missing());
        };
        if forbidden.contains(&tab_id) {
            clear();
            return Err(Problem::ApplicantTab);
        }
        let cells = match self
            .live
            .tab_cells(&s.spreadsheet_id, tab_id, &forbidden)
            .await
        {
            Ok(c) => c,
            Err(ReadError::TabNotFound) => {
                clear();
                return Err(missing());
            }
            Err(ReadError::NotShared) => return Err(Problem::SheetUnreadable),
            Err(ReadError::Google(_)) => return Err(Problem::Busy),
        };
        let (addrs, skipped) = extract_addresses(&cells);
        self.set_status(&s.id, |st| {
            st.recipients = Some(addrs.len());
            st.skipped = skipped;
        });
        if skipped > 0 {
            tracing::info!(
                "[portal notify] 送り先のタブで、メールアドレスの形でないものを {skipped} 件飛ばした"
            );
        }
        if addrs.is_empty() {
            return Err(Problem::NoRecipients);
        }
        if addrs.len() > MAX_RECIPIENTS {
            return Err(Problem::TooManyRecipients);
        }
        Ok(addrs)
    }

    fn addr_key(&self, s: &SheetReg, addr: &str) -> String {
        hex16(&hmac_sha256(
            &self.key,
            format!("a1\u{1f}{}\u{1f}{addr}", s.id).as_bytes(),
        ))
    }

    /// 未送信の行を知らせる。宛先ごとに 1 通。届かなかった宛先が残った行の数を返す
    async fn deliver(
        &self,
        s: &SheetReg,
        all: &[SheetReg],
        pending: &[NotifiedRow],
        row_of: &HashMap<&str, u32>,
    ) -> Result<usize, Problem> {
        let store = self.auth.store();
        // 送る直前に、シートと顧客が止められていないかを読み直す
        let Some(s) = store.sheet(&s.id) else {
            return Err(Problem::SheetStopped);
        };
        if s.disabled {
            return Err(Problem::SheetStopped);
        }
        let Some(customer) = store.customer(&s.customer_id).filter(|c| c.usable()) else {
            return Err(Problem::CustomerUnusable);
        };
        let to = self.recipients(&s, all).await?;
        let keyed: Vec<(String, String)> = to
            .iter()
            .map(|a| (a.clone(), self.addr_key(&s, a)))
            .collect();
        // 行ごとに送る宛先: 未送信なら全員、一部に届かなかった行はその宛先だけ (送り先から外れた宛先は送らない)
        let needs: Vec<Vec<&str>> = pending
            .iter()
            .map(|r| {
                keyed
                    .iter()
                    .filter(|(_, k)| r.remaining.is_empty() || r.remaining.contains(k))
                    .map(|(a, _)| a.as_str())
                    .collect()
            })
            .collect();
        let send_to: Vec<String> = to
            .iter()
            .filter(|a| needs.iter().any(|n| n.contains(&a.as_str())))
            .cloned()
            .collect();
        let now = Utc::now();
        let delivered: HashSet<String> = if send_to.is_empty() {
            HashSet::new()
        } else {
            let ok = self.limits.lock().unwrap_or_else(|e| e.into_inner()).take(
                &self.cfg,
                &s.id,
                send_to.len(),
                Instant::now(),
            );
            if !ok {
                return Err(Problem::RateLimited);
            }
            let (subject, body) = notify_message(&customer.name, &self.portal_url);
            self.mailer
                .send(&send_to, &subject, &body)
                .await
                .map_err(|_| Problem::MailFailed)?
                .into_iter()
                .collect()
        };
        let mut done_rows = Vec::new();
        let mut left_rows = 0;
        let updates: Vec<NotifiedRow> = pending
            .iter()
            .zip(&needs)
            .map(|(r, need)| {
                let left: Vec<String> = need
                    .iter()
                    .filter(|a| !delivered.contains(**a))
                    .map(|a| self.addr_key(&s, a))
                    .collect();
                if left.is_empty() {
                    if let Some(n) = row_of.get(r.fingerprint.as_str()) {
                        done_rows.push(*n);
                    }
                    NotifiedRow {
                        state: NotifyState::Sent,
                        at: now,
                        remaining: Vec::new(),
                        ..r.clone()
                    }
                } else {
                    left_rows += 1;
                    NotifiedRow {
                        state: NotifyState::Pending,
                        remaining: left,
                        ..r.clone()
                    }
                }
            })
            .collect();
        store.put_notified(updates);
        if !delivered.is_empty() {
            done_rows.sort_unstable();
            let mut e = LogEntry::new(now, "system", LogSource::System, "notify");
            e.customer_id = s.customer_id.clone();
            e.sheet_reg_id = s.id.clone();
            e.row = done_rows
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let failed = send_to.len() - delivered.len();
            let sent: Vec<&str> = send_to
                .iter()
                .filter(|a| delivered.contains(*a))
                .map(String::as_str)
                .collect();
            e.detail = format!(
                "新しい応募の通知を送った（新しい行 {} 件・{} 通{}）。送り先: {}",
                pending.len(),
                delivered.len(),
                if failed > 0 {
                    format!("。{failed} 通は届かず、次の確認で送り直す")
                } else {
                    String::new()
                },
                sent.join(", ")
            );
            store.append_log(e);
            tracing::info!(
                "[portal notify] 新しい応募の通知を送った（新しい行 {} 件・{} 通・届かなかった {failed} 通）",
                pending.len(),
                delivered.len()
            );
            self.set_status(&s.id, |st| st.last_sent_at = Some(now));
        }
        Ok(left_rows)
    }
}
