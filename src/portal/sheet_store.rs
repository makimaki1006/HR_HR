//! 社内用スプレッドシートを保存先にする [`PortalStore`] (計画 9 章、PR1)。
//!
//! 整合の取り方 (単一インスタンス前提。計画 9.2 に同じことを書いている):
//! - 起動時にシートの全タブ (操作の記録を除く) を 1 回で読み、メモリに持つ。読み取りは全部メモリから。
//!   読み終えるまでは [`PortalStore::ready`] が false で、画面・API は 503 を返す。
//! - 書き込みはまずメモリに入れ (その場で効く。停止・ログアウトはすぐ効く)、変わった行を覚えておく。
//!   裏の 1 本の処理が、まとめて (最短 [`SheetStoreConfig::min_gap`] おき) シートへ書く。
//!   書く前に鍵の列を読み直して行の位置を引き直すので、人が行を並べ替えても別の行には書かない。
//!   行の削除は行を空にする (行を消さない)。
//! - 書けなかったら、変わった行を覚えたまま待って再試行する (最長 [`SheetStoreConfig::retry_max`] おき)。
//!   管理画面に「保存待ち」の件数と理由を出す。保存待ちのまま再起動すると、その変更は失われる。
//! - [`SheetStoreConfig::reload_every`] おきにシートを読み直し、社内の人がシートで直した値を取り込む
//!   (保存待ちの行はメモリの方を残す)。シートから消えた行はメモリからも消す (アカウントなら使えなくなる)。
//! - 操作の記録は追記だけ。起動時には読まない (プロセス内には直近の分だけ持つ)。
//! - 読めない値は、開く側でなく閉じる側に倒す (知らない状態は「停止」、読めない期限の招待・端末は捨てる)。
//!
//! ログには件数・タブの名前・Google の HTTP の状態だけを出し、値・鍵・メールアドレスは出さない。
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
use tokio::sync::{watch, Notify};

use super::sheets::{a1, col_letter, GoogleError, SheetsApi};
use super::store::*;

// ---- タブと見出し ----

pub const TAB_CUSTOMERS: &str = "顧客";
pub const TAB_SHEETS: &str = "シート";
pub const TAB_COLUMNS: &str = "項目の設定";
pub const TAB_ACCOUNTS: &str = "アカウント";
pub const TAB_INVITES: &str = "招待";
pub const TAB_DEVICES: &str = "端末";
pub const TAB_SESSIONS: &str = "ログイン";
pub const TAB_LOG: &str = "操作の記録";

pub const H_CUSTOMERS: &[&str] = &[
    "顧客の番号",
    "社内での表示名",
    "HubSpotの会社ID",
    "段階",
    "停止",
    "作成日時",
    "更新日時",
];
pub const H_SHEETS: &[&str] = &[
    "シートの番号",
    "顧客の番号",
    "スプレッドシートID",
    "タブの番号",
    "タブの名前",
    "顧客に見せる名前",
    "見出しの行",
    "記入例の行数",
    "顧客に項目の変更を許す",
    "停止",
    "作成日時",
    "更新日時",
];
pub const H_COLUMNS: &[&str] = &[
    "シートの番号",
    "見出し",
    "画面での名前",
    "見せる",
    "書ける",
    "一覧に出す",
    "複数行",
    "種類",
    "まとまり",
    "並び",
];
pub const H_ACCOUNTS: &[&str] = &[
    "アカウントの番号",
    "メールアドレス",
    "顧客の番号",
    "パスワードのハッシュ",
    "確認コードの追加の送り先",
    "状態",
    "パスワードの変更日時",
    "最後のログイン",
    "作成日時",
];
pub const H_INVITES: &[&str] = &[
    "リンクのハッシュ",
    "アカウントの番号",
    "用途",
    "期限",
    "使った日時",
    "取り消し",
];
pub const H_DEVICES: &[&str] = &[
    "端末の鍵のハッシュ",
    "アカウントの番号",
    "作成日時",
    "最後に使った日時",
    "取り消し",
];
pub const H_SESSIONS: &[&str] = &["セッションのハッシュ", "アカウントの番号", "開始", "終了"];
pub const H_LOG: &[&str] = &[
    "日時",
    "操作者",
    "どこから",
    "種類",
    "顧客の番号",
    "シートの番号",
    "アカウントの番号",
    "応募者の行",
    "項目",
    "前の値",
    "後の値",
    "内容",
];

struct TabDef {
    table: Table,
    title: &'static str,
    headers: &'static [&'static str],
    /// 先頭の何列で行を特定するか
    key_cols: usize,
}

const TABS: &[TabDef] = &[
    TabDef {
        table: Table::Customers,
        title: TAB_CUSTOMERS,
        headers: H_CUSTOMERS,
        key_cols: 1,
    },
    TabDef {
        table: Table::Sheets,
        title: TAB_SHEETS,
        headers: H_SHEETS,
        key_cols: 1,
    },
    TabDef {
        table: Table::Columns,
        title: TAB_COLUMNS,
        headers: H_COLUMNS,
        key_cols: 2,
    },
    TabDef {
        table: Table::Accounts,
        title: TAB_ACCOUNTS,
        headers: H_ACCOUNTS,
        key_cols: 1,
    },
    TabDef {
        table: Table::Invites,
        title: TAB_INVITES,
        headers: H_INVITES,
        key_cols: 1,
    },
    TabDef {
        table: Table::Devices,
        title: TAB_DEVICES,
        headers: H_DEVICES,
        key_cols: 1,
    },
    TabDef {
        table: Table::Sessions,
        title: TAB_SESSIONS,
        headers: H_SESSIONS,
        key_cols: 1,
    },
];

fn def(table: Table) -> &'static TabDef {
    TABS.iter()
        .find(|d| d.table == table)
        .expect("全部の表に定義がある")
}

fn last_col(headers: &[&str]) -> String {
    col_letter(headers.len() - 1)
}

// ---- 値の書き方・読み方 ----

fn jst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("+09:00")
}

pub fn fmt_time(t: DateTime<Utc>) -> String {
    t.with_timezone(&jst())
        .to_rfc3339_opts(SecondsFormat::Millis, false)
}

fn fmt_opt(t: Option<DateTime<Utc>>) -> String {
    t.map(fmt_time).unwrap_or_default()
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn epoch() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH
}

fn flag(b: bool) -> String {
    if b { "TRUE" } else { "FALSE" }.to_string()
}

/// 停止・取り消しの列。空と FALSE 以外は「止めた」扱い (読めない値で開けない)
fn flag_closed(s: &str) -> bool {
    !matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "" | "false" | "0" | "いいえ"
    )
}

/// 見せる・書けるの列。TRUE・はい・1 のときだけ許す
fn flag_strict(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "はい"
    )
}

fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn cell(row: &[String], i: usize) -> &str {
    row.get(i).map(String::as_str).unwrap_or("")
}

fn encode(t: &Tables, table: Table, key: &str) -> Option<Vec<String>> {
    let v = match table {
        Table::Customers => {
            let c = t.customers.get(key)?;
            vec![
                c.id.clone(),
                c.name.clone(),
                c.hubspot_company_id.clone().unwrap_or_default(),
                c.stage.as_str().into(),
                flag(c.disabled),
                fmt_time(c.created_at),
                fmt_time(c.updated_at),
            ]
        }
        Table::Sheets => {
            let s = t.sheets.get(key)?;
            vec![
                s.id.clone(),
                s.customer_id.clone(),
                s.spreadsheet_id.clone(),
                s.sheet_id.to_string(),
                s.tab_title.clone(),
                s.label.clone(),
                s.header_row.to_string(),
                s.example_rows.to_string(),
                flag(s.customer_can_edit_columns),
                flag(s.disabled),
                fmt_time(s.created_at),
                fmt_time(s.updated_at),
            ]
        }
        Table::Columns => {
            let c = t.columns.get(key)?;
            vec![
                c.sheet_reg_id.clone(),
                c.header.clone(),
                c.label.clone(),
                flag(c.visible),
                flag(c.writable),
                flag(c.in_list),
                flag(c.multiline),
                c.kind.as_str().into(),
                c.group.as_str().into(),
                c.order.to_string(),
            ]
        }
        Table::Accounts => {
            let a = t.accounts.get(key)?;
            vec![
                a.id.clone(),
                a.email.clone(),
                a.customer_id.clone(),
                a.password_hash.clone().unwrap_or_default(),
                a.extra_code_email.clone().unwrap_or_default(),
                a.status.as_str().into(),
                fmt_opt(a.password_changed_at),
                fmt_opt(a.last_login_at),
                fmt_time(a.created_at),
            ]
        }
        Table::Invites => {
            let i = t.invites.get(key)?;
            vec![
                i.token_hash.clone(),
                i.account_id.clone(),
                i.purpose.as_str().into(),
                fmt_time(i.expires_at),
                fmt_opt(i.used_at),
                flag(i.revoked),
            ]
        }
        Table::Devices => {
            let d = t.devices.get(key)?;
            vec![
                d.token_hash.clone(),
                d.account_id.clone(),
                fmt_time(d.created_at),
                fmt_time(d.last_used_at),
                flag(d.revoked),
            ]
        }
        Table::Sessions => {
            let s = t.sessions.get(key)?;
            vec![
                s.token_hash.clone(),
                s.account_id.clone(),
                fmt_time(s.created_at),
                fmt_opt(s.ended_at),
            ]
        }
    };
    Some(v)
}

fn encode_log(e: &LogEntry) -> Vec<String> {
    vec![
        fmt_time(e.at),
        e.actor.clone(),
        e.source.as_str().into(),
        e.kind.clone(),
        e.customer_id.clone(),
        e.sheet_reg_id.clone(),
        e.account_id.clone(),
        e.row.clone(),
        e.column.clone(),
        e.before.clone(),
        e.after.clone(),
        e.detail.clone(),
    ]
}

/// 期限や状態の時刻が読めないときは「とても先」にし、パスワードを変えた扱いで全部のログインを無効にする
fn changed_at(s: &str) -> Option<DateTime<Utc>> {
    if s.trim().is_empty() {
        None
    } else {
        Some(parse_time(s).unwrap_or(DateTime::<Utc>::MAX_UTC))
    }
}

/// 1 行を読んで表に入れる。読めない行は入れずに false。同じ鍵が先にあれば入れずに false
fn decode_into(t: &mut Tables, table: Table, r: &[String]) -> bool {
    let id = cell(r, 0).trim().to_string();
    if id.is_empty() {
        return false;
    }
    match table {
        Table::Customers => {
            let name = cell(r, 1).trim();
            if name.is_empty() || t.customers.contains_key(&id) {
                return false;
            }
            t.customers.insert(
                id.clone(),
                Customer {
                    id,
                    name: name.to_string(),
                    hubspot_company_id: opt_str(cell(r, 2)),
                    stage: CustomerStage::parse(cell(r, 3)),
                    disabled: flag_closed(cell(r, 4)),
                    created_at: parse_time(cell(r, 5)).unwrap_or_else(epoch),
                    updated_at: parse_time(cell(r, 6)).unwrap_or_else(epoch),
                },
            );
        }
        Table::Sheets => {
            let (Ok(sheet_id), Ok(header_row)) =
                (cell(r, 3).trim().parse(), cell(r, 6).trim().parse::<u32>())
            else {
                return false;
            };
            let customer_id = cell(r, 1).trim();
            let spreadsheet_id = cell(r, 2).trim();
            if customer_id.is_empty()
                || spreadsheet_id.is_empty()
                || header_row == 0
                || t.sheets.contains_key(&id)
            {
                return false;
            }
            t.sheets.insert(
                id.clone(),
                SheetReg {
                    id,
                    customer_id: customer_id.into(),
                    spreadsheet_id: spreadsheet_id.into(),
                    sheet_id,
                    tab_title: cell(r, 4).to_string(),
                    label: cell(r, 5).to_string(),
                    header_row,
                    example_rows: cell(r, 7).trim().parse().unwrap_or(0),
                    customer_can_edit_columns: flag_strict(cell(r, 8)),
                    disabled: flag_closed(cell(r, 9)),
                    created_at: parse_time(cell(r, 10)).unwrap_or_else(epoch),
                    updated_at: parse_time(cell(r, 11)).unwrap_or_else(epoch),
                },
            );
        }
        Table::Columns => {
            let header = cell(r, 1).trim();
            let k = column_key(&id, header);
            if header.is_empty() || t.columns.contains_key(&k) {
                return false;
            }
            let visible = flag_strict(cell(r, 3));
            t.columns.insert(
                k,
                ColumnSetting {
                    sheet_reg_id: id,
                    header: header.into(),
                    label: cell(r, 2).to_string(),
                    visible,
                    // 見せない列は書かせない
                    writable: visible && flag_strict(cell(r, 4)),
                    in_list: flag_strict(cell(r, 5)),
                    multiline: flag_strict(cell(r, 6)),
                    kind: ColumnKind::parse(cell(r, 7)).unwrap_or(ColumnKind::Text),
                    group: ColumnGroup::parse(cell(r, 8)).unwrap_or(ColumnGroup::Other),
                    order: cell(r, 9).trim().parse().unwrap_or(9999),
                },
            );
        }
        Table::Accounts => {
            let email = cell(r, 1).trim().to_lowercase();
            let customer_id = cell(r, 2).trim();
            if email.is_empty() || customer_id.is_empty() || t.accounts.contains_key(&id) {
                return false;
            }
            t.accounts.insert(
                id.clone(),
                Account {
                    id,
                    email,
                    customer_id: customer_id.into(),
                    password_hash: opt_str(cell(r, 3)),
                    extra_code_email: opt_str(cell(r, 4)).map(|e| e.to_lowercase()),
                    status: AccountStatus::parse(cell(r, 5)),
                    password_changed_at: changed_at(cell(r, 6)),
                    last_login_at: parse_time(cell(r, 7)),
                    created_at: parse_time(cell(r, 8)).unwrap_or_else(epoch),
                },
            );
        }
        Table::Invites => {
            let (Some(purpose), Some(expires_at)) =
                (InvitePurpose::parse(cell(r, 2)), parse_time(cell(r, 3)))
            else {
                return false;
            };
            if t.invites.contains_key(&id) {
                return false;
            }
            let used = cell(r, 4);
            t.invites.insert(
                id.clone(),
                Invite {
                    token_hash: id,
                    account_id: cell(r, 1).trim().into(),
                    purpose,
                    expires_at,
                    // 読めない使用日時は使用済みにする
                    used_at: opt_str(used).map(|u| parse_time(&u).unwrap_or_else(epoch)),
                    revoked: flag_closed(cell(r, 5)),
                },
            );
        }
        Table::Devices => {
            let (Some(created_at), Some(last_used_at)) =
                (parse_time(cell(r, 2)), parse_time(cell(r, 3)))
            else {
                return false;
            };
            if t.devices.contains_key(&id) {
                return false;
            }
            t.devices.insert(
                id.clone(),
                Device {
                    token_hash: id,
                    account_id: cell(r, 1).trim().into(),
                    created_at,
                    last_used_at,
                    revoked: flag_closed(cell(r, 4)),
                },
            );
        }
        Table::Sessions => {
            let Some(created_at) = parse_time(cell(r, 2)) else {
                return false;
            };
            if t.sessions.contains_key(&id) {
                return false;
            }
            let ended = cell(r, 3);
            t.sessions.insert(
                id.clone(),
                Session {
                    token_hash: id,
                    account_id: cell(r, 1).trim().into(),
                    created_at,
                    // 読めない終了日時は終わった扱い
                    ended_at: opt_str(ended).map(|e| parse_time(&e).unwrap_or_else(epoch)),
                },
            );
        }
    }
    true
}

/// 行を特定する鍵 (先頭の `key_cols` 列)
fn row_key(d: &TabDef, r: &[String]) -> String {
    if d.key_cols == 2 {
        column_key(cell(r, 0).trim(), cell(r, 1).trim())
    } else {
        cell(r, 0).trim().to_string()
    }
}

// ---- 保存先 ----

/// まだ書いていない記録の上限
const MAX_PENDING_LOGS: usize = 5000;

#[derive(Debug, Clone)]
pub struct SheetStoreConfig {
    /// 変更が来てから書くまで待つ時間 (まとめて書く)
    pub debounce: Duration,
    /// 書き込みどうしの最短の間隔
    pub min_gap: Duration,
    /// シートを読み直す間隔
    pub reload_every: Duration,
    /// 失敗したときの再試行の最長の間隔
    pub retry_max: Duration,
}

impl Default for SheetStoreConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(500),
            min_gap: Duration::from_secs(3),
            reload_every: Duration::from_secs(5 * 60),
            retry_max: Duration::from_secs(60),
        }
    }
}

#[derive(Default)]
struct St {
    t: Tables,
    /// 変わった行 → 変えたときの番号
    dirty: HashMap<(Table, String), u64>,
    /// まだ書いていない記録
    logs: Vec<(u64, LogEntry)>,
    gen: u64,
    ready: bool,
    loaded_at: Option<DateTime<Utc>>,
    last_saved_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

pub struct SheetStore {
    api: Arc<dyn SheetsApi>,
    spreadsheet: String,
    cfg: SheetStoreConfig,
    st: Mutex<St>,
    notify: Notify,
    /// 書き終えた変更の番号
    flushed: watch::Sender<u64>,
}

/// 画面に出す失敗の文 (値・鍵は含めない)
pub fn describe(e: GoogleError) -> String {
    match e {
        GoogleError::Status(403 | 404) => {
            "社内用シートを開けません。サービスアカウントに編集者として共有してください".into()
        }
        GoogleError::Status(429) => {
            "Google の回数の上限に当たりました。少し待って再試行します".into()
        }
        GoogleError::Status(s) => format!("社内用シートの読み書きに失敗しました（{s}）"),
        GoogleError::Network => "Google につながりません。再試行します".into(),
        GoogleError::Token => "Google の認証に失敗しました。鍵を確かめてください".into(),
    }
}

fn log_failure(what: &str, e: GoogleError) {
    match e {
        GoogleError::Status(s) => tracing::warn!("[portal store] {what}の失敗（{s}）"),
        GoogleError::Network => tracing::warn!("[portal store] {what}の失敗（通信）"),
        GoogleError::Token => tracing::warn!("[portal store] {what}の失敗（認証）"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Google(GoogleError),
    /// そのタブの見出しが想定と違う (人が列を足し引きした)
    Headers(&'static str),
}

impl LoadError {
    fn message(&self) -> String {
        match self {
            LoadError::Google(e) => describe(*e),
            LoadError::Headers(tab) => {
                format!(
                    "社内用シートの「{tab}」の見出しが想定と違います。1 行目を元に戻してください"
                )
            }
        }
    }
}

impl From<GoogleError> for LoadError {
    fn from(e: GoogleError) -> Self {
        LoadError::Google(e)
    }
}

enum HeaderCheck {
    Ok,
    /// 空、または想定の先頭だけがある (列を足した版に上げる)。見出しの行を書く
    Write,
    Mismatch,
}

fn check_headers(actual: Option<&Vec<String>>, expected: &[&str]) -> HeaderCheck {
    let actual: Vec<&str> = actual
        .map(|r| r.iter().map(|s| s.trim()).collect())
        .unwrap_or_default();
    let used: Vec<&str> = {
        let mut v = actual.clone();
        while v.last().is_some_and(|s| s.is_empty()) {
            v.pop();
        }
        v
    };
    if used.len() >= expected.len() && used[..expected.len()] == *expected {
        return HeaderCheck::Ok;
    }
    if used.len() < expected.len() && used[..] == expected[..used.len()] {
        return HeaderCheck::Write;
    }
    HeaderCheck::Mismatch
}

impl SheetStore {
    /// 作るだけ (読み込みは [`SheetStore::load`])。テスト用
    pub fn new(api: Arc<dyn SheetsApi>, spreadsheet: &str, cfg: SheetStoreConfig) -> Arc<Self> {
        Arc::new(Self {
            api,
            spreadsheet: spreadsheet.to_string(),
            cfg,
            st: Mutex::new(St::default()),
            notify: Notify::new(),
            flushed: watch::channel(0).0,
        })
    }

    /// 作り、裏で読み込みと書き戻しを始める (今の tokio の上で)
    pub fn start(api: Arc<dyn SheetsApi>, spreadsheet: &str, cfg: SheetStoreConfig) -> Arc<Self> {
        let s = Self::new(api, spreadsheet, cfg);
        match tokio::runtime::Handle::try_current() {
            Ok(h) => {
                h.spawn(s.clone().run());
            }
            Err(_) => {
                tracing::warn!("[portal store] 非同期の実行環境が無いので社内用シートを読めない")
            }
        }
        s
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn mark(&self, changed: Changed) {
        if changed.is_empty() {
            return;
        }
        {
            let mut st = self.lock();
            st.gen += 1;
            let g = st.gen;
            for k in changed {
                st.dirty.insert(k, g);
            }
        }
        self.notify.notify_one();
    }

    fn has_pending(&self) -> bool {
        let st = self.lock();
        !st.dirty.is_empty() || !st.logs.is_empty()
    }

    fn data_ranges() -> Vec<String> {
        TABS.iter()
            .map(|d| a1(d.title, &format!("A1:{}", last_col(d.headers))))
            .collect()
    }

    /// シートの全タブを読み、表を作る。無いタブは作り、空・古い見出しは書く
    async fn read_all(&self, setup: bool) -> Result<Tables, LoadError> {
        if setup {
            let have: Vec<String> = self
                .api
                .tabs(&self.spreadsheet)
                .await?
                .into_iter()
                .map(|t| t.title)
                .collect();
            let missing: Vec<String> = TABS
                .iter()
                .map(|d| d.title)
                .chain([TAB_LOG])
                .filter(|t| !have.iter().any(|h| h == t))
                .map(str::to_string)
                .collect();
            if !missing.is_empty() {
                tracing::info!(
                    "[portal store] 社内用シートにタブを足す（{} 個）",
                    missing.len()
                );
                self.api.add_tabs(&self.spreadsheet, &missing).await?;
            }
        }
        let mut ranges = Self::data_ranges();
        ranges.push(a1(TAB_LOG, &format!("A1:{}1", last_col(H_LOG))));
        let got = self.api.batch_get(&self.spreadsheet, &ranges).await?;

        let mut header_writes: Vec<(String, Vec<Vec<String>>)> = Vec::new();
        let mut t = Tables::default();
        let mut skipped = 0usize;
        for (i, d) in TABS.iter().enumerate() {
            let rows = got.get(i).cloned().unwrap_or_default();
            match check_headers(rows.first(), d.headers) {
                HeaderCheck::Ok => {}
                HeaderCheck::Write => header_writes.push((
                    a1(d.title, &format!("A1:{}1", last_col(d.headers))),
                    vec![d.headers.iter().map(|s| s.to_string()).collect()],
                )),
                HeaderCheck::Mismatch => return Err(LoadError::Headers(d.title)),
            }
            for r in rows.iter().skip(1) {
                if r.iter().all(|c| c.trim().is_empty()) {
                    continue;
                }
                if !decode_into(&mut t, d.table, r) {
                    skipped += 1;
                }
            }
        }
        let log_rows = got.get(TABS.len()).cloned().unwrap_or_default();
        match check_headers(log_rows.first(), H_LOG) {
            HeaderCheck::Ok => {}
            HeaderCheck::Write => header_writes.push((
                a1(TAB_LOG, &format!("A1:{}1", last_col(H_LOG))),
                vec![H_LOG.iter().map(|s| s.to_string()).collect()],
            )),
            HeaderCheck::Mismatch => return Err(LoadError::Headers(TAB_LOG)),
        }
        if !header_writes.is_empty() {
            self.api
                .batch_update(&self.spreadsheet, &header_writes)
                .await?;
        }
        if skipped > 0 {
            tracing::warn!("[portal store] 読めない・重なった行を {skipped} 行とばした");
        }
        Ok(t)
    }

    /// 起動時の読み込み。成功したら使えるようにする
    pub async fn load(&self) -> Result<(), LoadError> {
        match self.read_all(true).await {
            Ok(t) => {
                let mut st = self.lock();
                let logs = std::mem::take(&mut st.t.logs);
                st.t = t;
                st.t.logs = logs;
                st.ready = true;
                st.loaded_at = Some(Utc::now());
                st.last_error = None;
                tracing::info!(
                    "[portal store] 社内用シートを読んだ（顧客 {}・シート {}・アカウント {}）",
                    st.t.customers.len(),
                    st.t.sheets.len(),
                    st.t.accounts.len()
                );
                Ok(())
            }
            Err(e) => {
                if let LoadError::Google(g) = &e {
                    log_failure("社内用シートの読み込み", *g);
                } else {
                    tracing::warn!("[portal store] 社内用シートの見出しが想定と違う");
                }
                self.lock().last_error = Some(e.message());
                Err(e)
            }
        }
    }

    /// 読み直して取り込む。保存待ちの行はメモリの方を残す
    pub async fn reload(&self) -> Result<(), LoadError> {
        let mut fresh = match self.read_all(false).await {
            Ok(t) => t,
            Err(e) => {
                if let LoadError::Google(g) = &e {
                    log_failure("社内用シートの読み直し", *g);
                }
                self.lock().last_error = Some(e.message());
                return Err(e);
            }
        };
        let mut st = self.lock();
        fn keep<V: Clone>(
            fresh: &mut HashMap<String, V>,
            old: &HashMap<String, V>,
            table: Table,
            dirty: &HashMap<(Table, String), u64>,
        ) {
            for (t, k) in dirty.keys() {
                if *t != table {
                    continue;
                }
                match old.get(k) {
                    Some(v) => {
                        fresh.insert(k.clone(), v.clone());
                    }
                    None => {
                        fresh.remove(k);
                    }
                }
            }
        }
        let St { t: old, dirty, .. } = &*st;
        keep(
            &mut fresh.customers,
            &old.customers,
            Table::Customers,
            dirty,
        );
        keep(&mut fresh.sheets, &old.sheets, Table::Sheets, dirty);
        keep(&mut fresh.columns, &old.columns, Table::Columns, dirty);
        keep(&mut fresh.accounts, &old.accounts, Table::Accounts, dirty);
        keep(&mut fresh.invites, &old.invites, Table::Invites, dirty);
        keep(&mut fresh.devices, &old.devices, Table::Devices, dirty);
        keep(&mut fresh.sessions, &old.sessions, Table::Sessions, dirty);
        fresh.logs = std::mem::take(&mut st.t.logs);
        st.t = fresh;
        st.loaded_at = Some(Utc::now());
        if st.dirty.is_empty() {
            st.last_error = None;
        }
        Ok(())
    }

    /// 保存待ちを 1 回書く。鍵の列を読み直して行の位置を引き、まとめて 1 回で書く。記録は追記で 1 回
    pub async fn flush_once(&self) -> Result<(), GoogleError> {
        let (snapshot, rows, logs) = {
            let mut st = self.lock();
            let snapshot = st.gen;
            let rows: Vec<(Table, String, Option<Vec<String>>)> = st
                .dirty
                .keys()
                .map(|(table, key)| (*table, key.clone(), encode(&st.t, *table, key)))
                .collect();
            let logs = std::mem::take(&mut st.logs);
            (snapshot, rows, logs)
        };
        if rows.is_empty() && logs.is_empty() {
            return Ok(());
        }
        let result = self.write(&rows, &logs).await;
        let mut st = self.lock();
        match result {
            Ok(()) => {
                st.dirty.retain(|_, g| *g > snapshot);
                st.last_saved_at = Some(Utc::now());
                st.last_error = None;
                drop(st);
                self.flushed.send_replace(snapshot);
                tracing::info!(
                    "[portal store] 社内用シートに書いた（{} 行・記録 {} 件）",
                    rows.len(),
                    logs.len()
                );
                Ok(())
            }
            Err((wrote_rows, e)) => {
                if wrote_rows {
                    st.dirty.retain(|_, g| *g > snapshot);
                }
                // 記録は前に戻す (順番を保つ)
                let mut back = logs;
                back.append(&mut st.logs);
                st.logs = back;
                st.last_error = Some(describe(e));
                log_failure("社内用シートへの書き込み", e);
                Err(e)
            }
        }
    }

    async fn write(
        &self,
        rows: &[(Table, String, Option<Vec<String>>)],
        logs: &[(u64, LogEntry)],
    ) -> Result<(), (bool, GoogleError)> {
        if !rows.is_empty() {
            let mut tables: Vec<Table> = rows.iter().map(|r| r.0).collect();
            tables.sort();
            tables.dedup();
            let ranges: Vec<String> = tables
                .iter()
                .map(|t| {
                    let d = def(*t);
                    a1(d.title, &format!("A2:{}", col_letter(d.key_cols - 1)))
                })
                .collect();
            let keys = self
                .api
                .batch_get(&self.spreadsheet, &ranges)
                .await
                .map_err(|e| (false, e))?;
            let mut data: Vec<(String, Vec<Vec<String>>)> = Vec::new();
            for (ti, table) in tables.iter().enumerate() {
                let d = def(*table);
                let col_end = last_col(d.headers);
                let existing = keys.get(ti).cloned().unwrap_or_default();
                let mut at: HashMap<String, Vec<usize>> = HashMap::new();
                for (i, r) in existing.iter().enumerate() {
                    let k = row_key(d, r);
                    if !k.is_empty() && k != column_key("", "") {
                        at.entry(k).or_default().push(i + 2);
                    }
                }
                let mut next = existing.len() + 2;
                for (_, key, row) in rows.iter().filter(|r| r.0 == *table) {
                    let found = at.get(key).cloned().unwrap_or_default();
                    match row {
                        Some(values) => {
                            let targets = if found.is_empty() {
                                let n = next;
                                next += 1;
                                vec![n]
                            } else {
                                // 同じ鍵の行が複数あれば全部を同じにする
                                found
                            };
                            for n in targets {
                                data.push((
                                    a1(d.title, &format!("A{n}:{col_end}{n}")),
                                    vec![values.clone()],
                                ));
                            }
                        }
                        None => {
                            for n in found {
                                data.push((
                                    a1(d.title, &format!("A{n}:{col_end}{n}")),
                                    vec![vec![String::new(); d.headers.len()]],
                                ));
                            }
                        }
                    }
                }
            }
            if !data.is_empty() {
                self.api
                    .batch_update(&self.spreadsheet, &data)
                    .await
                    .map_err(|e| (false, e))?;
            }
        }
        if !logs.is_empty() {
            let values: Vec<Vec<String>> = logs.iter().map(|(_, e)| encode_log(e)).collect();
            self.api
                .append(
                    &self.spreadsheet,
                    &a1(TAB_LOG, &format!("A1:{}1", last_col(H_LOG))),
                    &values,
                )
                .await
                .map_err(|e| (true, e))?;
        }
        Ok(())
    }

    /// 起動時の読み込み (成功するまで再試行) のあと、書き戻しと読み直しを続ける
    pub async fn run(self: Arc<Self>) {
        let mut wait = Duration::from_secs(5);
        while self.load().await.is_err() {
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(Duration::from_secs(300));
        }
        let mut backoff: Option<Duration> = None;
        let mut last_flush = tokio::time::Instant::now() - self.cfg.min_gap;
        let mut next_reload = tokio::time::Instant::now() + self.cfg.reload_every;
        loop {
            let now = tokio::time::Instant::now();
            let sleep_for = if self.has_pending() {
                backoff.unwrap_or(self.cfg.debounce)
            } else {
                next_reload.saturating_duration_since(now)
            };
            tokio::select! {
                _ = self.notify.notified() => {
                    if backoff.is_none() {
                        tokio::time::sleep(self.cfg.debounce).await;
                    }
                }
                _ = tokio::time::sleep(sleep_for) => {}
            }
            if self.has_pending() {
                let since = last_flush.elapsed();
                if since < self.cfg.min_gap {
                    tokio::time::sleep(self.cfg.min_gap - since).await;
                }
                last_flush = tokio::time::Instant::now();
                match self.flush_once().await {
                    Ok(()) => backoff = None,
                    Err(_) => {
                        let b = backoff.map_or(Duration::from_secs(2), |b| b * 2);
                        backoff = Some(b.min(self.cfg.retry_max));
                    }
                }
            } else if tokio::time::Instant::now() >= next_reload {
                let _ = self.reload().await;
                next_reload = tokio::time::Instant::now() + self.cfg.reload_every;
            }
        }
    }
}

impl PortalStore for SheetStore {
    fn accounts(&self) -> Vec<Account> {
        self.lock().t.accounts()
    }
    fn account(&self, id: &str) -> Option<Account> {
        self.lock().t.accounts.get(id).cloned()
    }
    fn account_by_email(&self, email: &str) -> Option<Account> {
        self.lock().t.account_by_email(email)
    }
    fn insert_account(&self, account: Account) -> Result<(), StoreError> {
        let c = self.lock().t.insert_account(account)?;
        self.mark(c);
        Ok(())
    }
    fn update_account(&self, account: Account) -> Result<(), StoreError> {
        let c = self.lock().t.update_account(account)?;
        self.mark(c);
        Ok(())
    }

    fn insert_invite(&self, invite: Invite) {
        let c = self.lock().t.insert_invite(invite);
        self.mark(c);
    }
    fn invite(&self, token_hash: &str) -> Option<Invite> {
        self.lock().t.invites.get(token_hash).cloned()
    }
    fn use_invite(&self, token_hash: &str, now: DateTime<Utc>) -> bool {
        let c = self.lock().t.use_invite(token_hash, now);
        match c {
            Some(c) => {
                self.mark(c);
                true
            }
            None => false,
        }
    }
    fn revoke_invites(&self, account_id: &str) {
        let c = self.lock().t.revoke_invites(account_id);
        self.mark(c);
    }

    fn insert_device(&self, device: Device) {
        let c = self.lock().t.insert_device(device);
        self.mark(c);
    }
    fn device(&self, token_hash: &str) -> Option<Device> {
        self.lock().t.devices.get(token_hash).cloned()
    }
    fn touch_device(&self, token_hash: &str, now: DateTime<Utc>) {
        let c = self.lock().t.touch_device(token_hash, now);
        self.mark(c);
    }
    fn revoke_devices(&self, account_id: &str) {
        let c = self.lock().t.revoke_devices(account_id);
        self.mark(c);
    }

    fn insert_session(&self, session: Session) {
        let c = self.lock().t.insert_session(session);
        self.mark(c);
    }
    fn session(&self, token_hash: &str) -> Option<Session> {
        self.lock().t.sessions.get(token_hash).cloned()
    }
    fn end_session(&self, token_hash: &str, now: DateTime<Utc>) {
        let c = self.lock().t.end_session(token_hash, now);
        self.mark(c);
    }
    fn end_sessions(&self, account_id: &str, now: DateTime<Utc>) {
        let c = self.lock().t.end_sessions(account_id, now);
        self.mark(c);
    }

    fn customers(&self) -> Vec<Customer> {
        self.lock().t.customers()
    }
    fn customer(&self, id: &str) -> Option<Customer> {
        self.lock().t.customers.get(id).cloned()
    }
    fn insert_customer(&self, customer: Customer) -> Result<(), StoreError> {
        let c = self.lock().t.insert_customer(customer)?;
        self.mark(c);
        Ok(())
    }
    fn update_customer(&self, customer: Customer) -> Result<(), StoreError> {
        let c = self.lock().t.update_customer(customer)?;
        self.mark(c);
        Ok(())
    }

    fn sheets(&self, customer_id: Option<&str>) -> Vec<SheetReg> {
        self.lock().t.sheets(customer_id)
    }
    fn sheet(&self, id: &str) -> Option<SheetReg> {
        self.lock().t.sheets.get(id).cloned()
    }
    fn insert_sheet(&self, sheet: SheetReg) -> Result<(), StoreError> {
        let c = self.lock().t.insert_sheet(sheet)?;
        self.mark(c);
        Ok(())
    }
    fn update_sheet(&self, sheet: SheetReg) -> Result<(), StoreError> {
        let c = self.lock().t.update_sheet(sheet)?;
        self.mark(c);
        Ok(())
    }

    fn columns(&self, sheet_reg_id: &str) -> Vec<ColumnSetting> {
        self.lock().t.columns(sheet_reg_id)
    }
    fn replace_columns(&self, sheet_reg_id: &str, columns: Vec<ColumnSetting>) {
        let c = self.lock().t.replace_columns(sheet_reg_id, columns);
        self.mark(c);
    }

    fn append_log(&self, entry: LogEntry) {
        {
            let mut st = self.lock();
            st.gen += 1;
            let g = st.gen;
            st.t.push_log(entry.clone());
            if st.logs.len() >= MAX_PENDING_LOGS {
                // シートに長く書けないとき、記録だけで膨らまないようにする (古いものから捨てる)
                st.logs.remove(0);
                tracing::warn!("[portal store] 書けていない記録が多すぎるので古いものを捨てた");
            }
            st.logs.push((g, entry));
        }
        self.notify.notify_one();
    }
    fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        self.lock().t.recent_logs(limit)
    }

    fn ready(&self) -> bool {
        self.lock().ready
    }
    fn status(&self) -> StoreStatus {
        let st = self.lock();
        StoreStatus {
            kind: "sheet",
            ready: st.ready,
            pending: st.dirty.len() + st.logs.len(),
            last_error: st.last_error.clone(),
            loaded_at: st.loaded_at,
            last_saved_at: st.last_saved_at,
        }
    }
    fn persisted(&self, timeout: Duration) -> PersistFuture<'_> {
        Box::pin(async move {
            let target = {
                let st = self.lock();
                if st.dirty.is_empty() && st.logs.is_empty() {
                    return true;
                }
                st.gen
            };
            self.notify.notify_one();
            let mut rx = self.flushed.subscribe();
            tokio::time::timeout(timeout, async move {
                loop {
                    if *rx.borrow_and_update() >= target {
                        return true;
                    }
                    if rx.changed().await.is_err() {
                        return false;
                    }
                }
            })
            .await
            .unwrap_or(false)
        })
    }
}
