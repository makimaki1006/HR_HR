//! 顧客向け応募者画面のアプリの情報の保存先。
//!
//! 計画 (`docs/architecture/customer-applicant-portal-plan.html` 9 章) では社内用のスプレッドシート 1 枚に置く。
//! 差し替えられる [`PortalStore`] と、プロセス内のメモリに置く [`MemoryStore`] (テストと手元の確認用) を持つ。
//! 社内用シートの実装は [`super::sheet_store::SheetStore`]。どちらも同じ [`Tables`] を使い、
//! 読み書きの決まり (二重登録の拒否・招待の 1 回だけの使用など) を 1 か所にまとめる。
//! 招待・端末・ログインはハッシュだけを置く。
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    /// 招待済みでパスワードをまだ決めていない
    Invited,
    Active,
    Disabled,
}

impl AccountStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AccountStatus::Invited => "invited",
            AccountStatus::Active => "active",
            AccountStatus::Disabled => "disabled",
        }
    }

    /// 知らない値は止めた扱い (シートを人が書き換えても、開く側に倒さない)
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "invited" => AccountStatus::Invited,
            "active" => AccountStatus::Active,
            _ => AccountStatus::Disabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    /// 小文字
    pub email: String,
    pub customer_id: String,
    pub password_hash: Option<String>,
    /// 確認コードの追加の送り先 (小文字)
    pub extra_code_email: Option<String>,
    pub status: AccountStatus,
    pub password_changed_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitePurpose {
    /// 初回のパスワード設定
    Initial,
    /// パスワードの再設定
    Reset,
}

impl InvitePurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            InvitePurpose::Initial => "initial",
            InvitePurpose::Reset => "reset",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "initial" => Some(InvitePurpose::Initial),
            "reset" => Some(InvitePurpose::Reset),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub token_hash: String,
    pub account_id: String,
    pub purpose: InvitePurpose,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    /// 新しい招待を出した・アカウントを止めたときに無効にする
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub token_hash: String,
    pub account_id: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub token_hash: String,
    pub account_id: String,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// 顧客ごとの移行の段階 (計画 6 章)。「シートだけ」の顧客は画面にログインできない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerStage {
    SheetOnly,
    Both,
    PortalOnly,
}

impl CustomerStage {
    pub fn as_str(self) -> &'static str {
        match self {
            CustomerStage::SheetOnly => "sheet_only",
            CustomerStage::Both => "both",
            CustomerStage::PortalOnly => "portal_only",
        }
    }

    /// 知らない値は「シートだけ」(画面を使わせない側) にする
    pub fn parse(s: &str) -> Self {
        Self::parse_known(s).unwrap_or(CustomerStage::SheetOnly)
    }

    pub fn parse_known(s: &str) -> Option<Self> {
        match s.trim() {
            "sheet_only" => Some(CustomerStage::SheetOnly),
            "both" => Some(CustomerStage::Both),
            "portal_only" => Some(CustomerStage::PortalOnly),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer {
    pub id: String,
    /// 社内での表示名
    pub name: String,
    pub hubspot_company_id: Option<String>,
    pub stage: CustomerStage,
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Customer {
    /// この顧客のアカウントが画面にログインしてよいか
    pub fn usable(&self) -> bool {
        !self.disabled && self.stage != CustomerStage::SheetOnly
    }
}

/// 顧客ごとに登録したシート (タブ 1 つ)。ブラウザからはこの番号 (`id`) だけを受け取り、
/// spreadsheetId は受け取らない (計画 6 章)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetReg {
    pub id: String,
    pub customer_id: String,
    pub spreadsheet_id: String,
    /// タブの番号 (名前の変更に強くする)
    pub sheet_id: i64,
    /// 最後に読んだときのタブの名前 (表示用。読み書きは番号で引き直す)
    pub tab_title: String,
    /// 顧客に見せる名前
    pub label: String,
    /// 見出しの行 (1 始まり)
    pub header_row: u32,
    /// 見出しの下の記入例の行数 (応募者ではない)
    pub example_rows: u32,
    pub customer_can_edit_columns: bool,
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// 応募の通知の送り先のタブの名前 (表示用。保存したときの名前)。空なら通知しない (計画 14 章)
    pub notify_tab: String,
    /// 応募の通知の送り先のタブの番号。送り先はこの番号で読む (名前の変更で別のタブに差し替わらない)
    pub notify_tab_id: Option<i64>,
}

/// 応募の通知の行の状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyState {
    /// 送り先の全員に届いた
    Sent,
    /// まだ送っていない、または一部の宛先に届いていない
    Pending,
}

impl NotifyState {
    pub fn as_str(self) -> &'static str {
        match self {
            NotifyState::Sent => "sent",
            NotifyState::Pending => "pending",
        }
    }
    /// 知らない値は「送った」(2 度送らない側に倒す)
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "pending" => NotifyState::Pending,
            _ => NotifyState::Sent,
        }
    }
}

/// 応募の通知の行 (計画 14 章)。同じ行に 2 度送らないため・送れなかった行を再起動後も送るために持つ。
/// 行の指紋と宛先は鍵付きのハッシュ (HMAC) だけを置き、生の値は置かない
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifiedRow {
    pub sheet_reg_id: String,
    /// 行の指紋の鍵付きハッシュ (16 進)
    pub fingerprint: String,
    pub state: NotifyState,
    /// 送った日時 (未送信なら見つけた日時)
    pub at: DateTime<Utc>,
    /// 一部の宛先に届かなかったとき、その宛先の鍵付きハッシュ。空で未送信なら送り先の全員
    pub remaining: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Date,
    Datetime,
    Phone,
    Email,
    Choice,
}

impl ColumnKind {
    pub const ALL: [ColumnKind; 6] = [
        ColumnKind::Text,
        ColumnKind::Date,
        ColumnKind::Datetime,
        ColumnKind::Phone,
        ColumnKind::Email,
        ColumnKind::Choice,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            ColumnKind::Text => "text",
            ColumnKind::Date => "date",
            ColumnKind::Datetime => "datetime",
            ColumnKind::Phone => "phone",
            ColumnKind::Email => "email",
            ColumnKind::Choice => "choice",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s.trim())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnGroup {
    Summary,
    Customer,
    Staff,
    Other,
}

impl ColumnGroup {
    pub const ALL: [ColumnGroup; 4] = [
        ColumnGroup::Summary,
        ColumnGroup::Customer,
        ColumnGroup::Staff,
        ColumnGroup::Other,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            ColumnGroup::Summary => "summary",
            ColumnGroup::Customer => "customer",
            ColumnGroup::Staff => "staff",
            ColumnGroup::Other => "other",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s.trim())
    }
}

/// シートの見出しごとの、画面での扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSetting {
    pub sheet_reg_id: String,
    /// シート上の見出し (元の名前)
    pub header: String,
    /// 画面での名前 (空なら見出し)
    pub label: String,
    pub visible: bool,
    pub writable: bool,
    pub in_list: bool,
    pub multiline: bool,
    pub kind: ColumnKind,
    pub group: ColumnGroup,
    /// 並び (0 始まり)
    pub order: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    /// 社内の管理画面
    Admin,
    /// 顧客の画面
    Portal,
    /// アプリ自身
    System,
}

impl LogSource {
    pub fn as_str(self) -> &'static str {
        match self {
            LogSource::Admin => "admin",
            LogSource::Portal => "portal",
            LogSource::System => "system",
        }
    }
}

/// 操作の記録の 1 件。追記だけ (書き換えない)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub at: DateTime<Utc>,
    /// 社内の人はメールアドレス、顧客はアカウントの番号
    pub actor: String,
    pub source: LogSource,
    pub kind: String,
    pub customer_id: String,
    pub sheet_reg_id: String,
    pub account_id: String,
    /// 応募者の行 (シートの行番号)。PR5 で使う
    pub row: String,
    pub column: String,
    pub before: String,
    pub after: String,
    pub detail: String,
}

impl LogEntry {
    pub fn new(at: DateTime<Utc>, actor: &str, source: LogSource, kind: &str) -> Self {
        Self {
            at,
            actor: actor.to_string(),
            source,
            kind: kind.to_string(),
            customer_id: String::new(),
            sheet_reg_id: String::new(),
            account_id: String::new(),
            row: String::new(),
            column: String::new(),
            before: String::new(),
            after: String::new(),
            detail: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// 同じメールアドレスのアカウント・同じシートのタブがある
    Duplicate,
    NotFound,
}

/// 保存先の状態 (社内の管理画面に出す)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreStatus {
    /// `memory` / `sheet`
    pub kind: &'static str,
    /// 読み込みが済み、使える
    pub ready: bool,
    /// シートにまだ書けていない変更の数
    pub pending: usize,
    /// 直近の失敗 (画面にそのまま出す文。値・鍵は含めない)
    pub last_error: Option<String>,
    pub loaded_at: Option<DateTime<Utc>>,
    pub last_saved_at: Option<DateTime<Utc>>,
}

pub type PersistFuture<'a> = Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

/// 保存先。読み取りは同期 (メモリから)。書き込みもメモリへは即時で、
/// 永続化 (社内用シート) は実装ごとに後から行う。[`PortalStore::persisted`] で書き終わりを待てる。
pub trait PortalStore: Send + Sync {
    fn accounts(&self) -> Vec<Account>;
    fn account(&self, id: &str) -> Option<Account>;
    fn account_by_email(&self, email: &str) -> Option<Account>;
    fn insert_account(&self, account: Account) -> Result<(), StoreError>;
    fn update_account(&self, account: Account) -> Result<(), StoreError>;
    /// 最後のログインの日時だけを変える (ほかの項目は今の値のまま。古い読み取りで上書きしない)
    fn touch_login(&self, account_id: &str, now: DateTime<Utc>);

    fn insert_invite(&self, invite: Invite);
    fn invite(&self, token_hash: &str) -> Option<Invite>;
    /// 未使用・未失効のときだけ使用済みにして true (同じリンクの同時使用で 2 回通さない)
    fn use_invite(&self, token_hash: &str, now: DateTime<Utc>) -> bool;
    fn revoke_invites(&self, account_id: &str);

    fn insert_device(&self, device: Device);
    fn device(&self, token_hash: &str) -> Option<Device>;
    fn touch_device(&self, token_hash: &str, now: DateTime<Utc>);
    fn revoke_devices(&self, account_id: &str);

    fn insert_session(&self, session: Session);
    fn session(&self, token_hash: &str) -> Option<Session>;
    fn end_session(&self, token_hash: &str, now: DateTime<Utc>);
    fn end_sessions(&self, account_id: &str, now: DateTime<Utc>);

    fn customers(&self) -> Vec<Customer>;
    fn customer(&self, id: &str) -> Option<Customer>;
    fn insert_customer(&self, customer: Customer) -> Result<(), StoreError>;
    fn update_customer(&self, customer: Customer) -> Result<(), StoreError>;

    /// `customer_id` が None なら全部
    fn sheets(&self, customer_id: Option<&str>) -> Vec<SheetReg>;
    fn sheet(&self, id: &str) -> Option<SheetReg>;
    /// 同じスプレッドシートの同じタブは、どの顧客にも二重に登録させない (別の顧客に見せない)
    fn insert_sheet(&self, sheet: SheetReg) -> Result<(), StoreError>;
    fn update_sheet(&self, sheet: SheetReg) -> Result<(), StoreError>;

    /// 並び順
    fn columns(&self, sheet_reg_id: &str) -> Vec<ColumnSetting>;
    /// そのシートの項目の設定を丸ごと置き換える
    fn replace_columns(&self, sheet_reg_id: &str, columns: Vec<ColumnSetting>);

    /// 応募の通知の行 (そのシートの分)
    fn notified(&self, sheet_reg_id: &str) -> Vec<NotifiedRow>;
    /// 応募の通知の行を足す・置き換える。「送った」行を「未送信」には戻さない
    fn put_notified(&self, rows: Vec<NotifiedRow>);
    /// `before` より古い応募の通知の行を消す。消した数
    fn purge_notified(&self, before: DateTime<Utc>) -> usize;

    fn append_log(&self, entry: LogEntry);
    /// 新しい順。プロセス内に残っている分だけ (古いものは社内用シートで見る)
    fn recent_logs(&self, limit: usize) -> Vec<LogEntry>;

    /// 読み込みが済んで使えるか。社内用シートは起動時に読み終えるまで false
    fn ready(&self) -> bool {
        true
    }
    fn status(&self) -> StoreStatus;
    /// ここまでの変更が永続化されるのを最大 `timeout` 待つ。書けたら true
    fn persisted(&self, _timeout: Duration) -> PersistFuture<'_> {
        Box::pin(async { true })
    }
}

/// 表の名前 (社内用シートのタブ)。変わった行を覚えるときの鍵に使う
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Table {
    Customers,
    Sheets,
    Columns,
    Accounts,
    Invites,
    Devices,
    Sessions,
    Notified,
}

/// 変わった行 (表と鍵)。社内用シートへ書き戻す対象
pub type Changed = Vec<(Table, String)>;

pub const RECENT_LOGS: usize = 500;

/// 項目の設定の鍵 (シートの番号と見出し)
pub fn column_key(sheet_reg_id: &str, header: &str) -> String {
    format!("{sheet_reg_id}\u{1f}{header}")
}

/// 表の中身と、読み書きの決まり。メモリの保存先と社内用シートの保存先で共用する。
/// 書き込みの関数は、変わった行を返す (社内用シートが書き戻しに使う)。
#[derive(Default, Debug, Clone)]
pub struct Tables {
    pub accounts: HashMap<String, Account>,
    pub invites: HashMap<String, Invite>,
    pub devices: HashMap<String, Device>,
    pub sessions: HashMap<String, Session>,
    pub customers: HashMap<String, Customer>,
    pub sheets: HashMap<String, SheetReg>,
    /// 鍵は [`column_key`]
    pub columns: HashMap<String, ColumnSetting>,
    /// 鍵は [`column_key`] (シートの番号と指紋のハッシュ)
    pub notified: HashMap<String, NotifiedRow>,
    pub logs: VecDeque<LogEntry>,
    /// 社内用シートで読めなかったマスの元の文字 (表・鍵・列)。書き戻すときはこの文字のまま書く
    pub unreadable: HashMap<(Table, String, usize), String>,
}

impl Tables {
    pub fn accounts(&self) -> Vec<Account> {
        let mut out: Vec<_> = self.accounts.values().cloned().collect();
        out.sort_by(|a, b| (&a.customer_id, &a.email).cmp(&(&b.customer_id, &b.email)));
        out
    }
    pub fn account_by_email(&self, email: &str) -> Option<Account> {
        // 同じメールの行が複数ある (シートを人が複製した) ときは、どれも使わない
        let mut it = self.accounts.values().filter(|a| a.email == email);
        let first = it.next()?.clone();
        it.next().is_none().then_some(first)
    }
    pub fn insert_account(&mut self, account: Account) -> Result<Changed, StoreError> {
        if self.accounts.values().any(|a| a.email == account.email)
            || self.accounts.contains_key(&account.id)
        {
            return Err(StoreError::Duplicate);
        }
        let key = account.id.clone();
        self.accounts.insert(key.clone(), account);
        Ok(vec![(Table::Accounts, key)])
    }
    pub fn update_account(&mut self, account: Account) -> Result<Changed, StoreError> {
        match self.accounts.get_mut(&account.id) {
            Some(a) => {
                let key = account.id.clone();
                *a = account;
                Ok(vec![(Table::Accounts, key)])
            }
            None => Err(StoreError::NotFound),
        }
    }

    pub fn touch_login(&mut self, account_id: &str, now: DateTime<Utc>) -> Changed {
        match self.accounts.get_mut(account_id) {
            Some(a) => {
                a.last_login_at = Some(now);
                vec![(Table::Accounts, account_id.to_string())]
            }
            None => Vec::new(),
        }
    }

    pub fn insert_invite(&mut self, invite: Invite) -> Changed {
        let key = invite.token_hash.clone();
        self.invites.insert(key.clone(), invite);
        vec![(Table::Invites, key)]
    }
    pub fn use_invite(&mut self, token_hash: &str, now: DateTime<Utc>) -> Option<Changed> {
        match self.invites.get_mut(token_hash) {
            Some(i) if i.used_at.is_none() && !i.revoked && now < i.expires_at => {
                i.used_at = Some(now);
                Some(vec![(Table::Invites, token_hash.to_string())])
            }
            _ => None,
        }
    }
    pub fn revoke_invites(&mut self, account_id: &str) -> Changed {
        let mut out = Vec::new();
        for i in self.invites.values_mut() {
            if i.account_id == account_id && !i.revoked {
                i.revoked = true;
                out.push((Table::Invites, i.token_hash.clone()));
            }
        }
        out
    }

    pub fn insert_device(&mut self, device: Device) -> Changed {
        let key = device.token_hash.clone();
        self.devices.insert(key.clone(), device);
        vec![(Table::Devices, key)]
    }
    pub fn touch_device(&mut self, token_hash: &str, now: DateTime<Utc>) -> Changed {
        match self.devices.get_mut(token_hash) {
            Some(d) => {
                d.last_used_at = now;
                vec![(Table::Devices, token_hash.to_string())]
            }
            None => Vec::new(),
        }
    }
    pub fn revoke_devices(&mut self, account_id: &str) -> Changed {
        let mut out = Vec::new();
        for d in self.devices.values_mut() {
            if d.account_id == account_id && !d.revoked {
                d.revoked = true;
                out.push((Table::Devices, d.token_hash.clone()));
            }
        }
        out
    }

    pub fn insert_session(&mut self, session: Session) -> Changed {
        let key = session.token_hash.clone();
        self.sessions.insert(key.clone(), session);
        vec![(Table::Sessions, key)]
    }
    pub fn end_session(&mut self, token_hash: &str, now: DateTime<Utc>) -> Changed {
        match self.sessions.get_mut(token_hash) {
            Some(s) if s.ended_at.is_none() => {
                s.ended_at = Some(now);
                vec![(Table::Sessions, token_hash.to_string())]
            }
            _ => Vec::new(),
        }
    }
    pub fn end_sessions(&mut self, account_id: &str, now: DateTime<Utc>) -> Changed {
        let mut out = Vec::new();
        for s in self.sessions.values_mut() {
            if s.account_id == account_id && s.ended_at.is_none() {
                s.ended_at = Some(now);
                out.push((Table::Sessions, s.token_hash.clone()));
            }
        }
        out
    }

    pub fn customers(&self) -> Vec<Customer> {
        let mut out: Vec<_> = self.customers.values().cloned().collect();
        out.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        out
    }
    pub fn insert_customer(&mut self, customer: Customer) -> Result<Changed, StoreError> {
        if self.customers.contains_key(&customer.id) {
            return Err(StoreError::Duplicate);
        }
        let key = customer.id.clone();
        self.customers.insert(key.clone(), customer);
        Ok(vec![(Table::Customers, key)])
    }
    pub fn update_customer(&mut self, customer: Customer) -> Result<Changed, StoreError> {
        match self.customers.get_mut(&customer.id) {
            Some(c) => {
                let key = customer.id.clone();
                *c = customer;
                Ok(vec![(Table::Customers, key)])
            }
            None => Err(StoreError::NotFound),
        }
    }

    pub fn sheets(&self, customer_id: Option<&str>) -> Vec<SheetReg> {
        let mut out: Vec<_> = self
            .sheets
            .values()
            .filter(|s| customer_id.is_none_or(|c| s.customer_id == c))
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            (&a.customer_id, a.created_at, &a.id).cmp(&(&b.customer_id, b.created_at, &b.id))
        });
        out
    }
    pub fn insert_sheet(&mut self, sheet: SheetReg) -> Result<Changed, StoreError> {
        if self.sheets.contains_key(&sheet.id)
            || self
                .sheets
                .values()
                .any(|s| s.spreadsheet_id == sheet.spreadsheet_id && s.sheet_id == sheet.sheet_id)
        {
            return Err(StoreError::Duplicate);
        }
        let key = sheet.id.clone();
        self.sheets.insert(key.clone(), sheet);
        Ok(vec![(Table::Sheets, key)])
    }
    pub fn update_sheet(&mut self, sheet: SheetReg) -> Result<Changed, StoreError> {
        let Some(old) = self.sheets.get(&sheet.id) else {
            return Err(StoreError::NotFound);
        };
        // 登録した先 (どの顧客の・どのシートか) は変えさせない
        if old.customer_id != sheet.customer_id
            || old.spreadsheet_id != sheet.spreadsheet_id
            || old.sheet_id != sheet.sheet_id
        {
            return Err(StoreError::Duplicate);
        }
        let key = sheet.id.clone();
        self.sheets.insert(key.clone(), sheet);
        Ok(vec![(Table::Sheets, key)])
    }

    pub fn columns(&self, sheet_reg_id: &str) -> Vec<ColumnSetting> {
        let mut out: Vec<_> = self
            .columns
            .values()
            .filter(|c| c.sheet_reg_id == sheet_reg_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| (a.order, &a.header).cmp(&(b.order, &b.header)));
        out
    }
    pub fn replace_columns(&mut self, sheet_reg_id: &str, columns: Vec<ColumnSetting>) -> Changed {
        let mut out = Vec::new();
        let old: Vec<String> = self
            .columns
            .iter()
            .filter(|(_, c)| c.sheet_reg_id == sheet_reg_id)
            .map(|(k, _)| k.clone())
            .collect();
        for k in old {
            self.columns.remove(&k);
            out.push((Table::Columns, k));
        }
        for mut c in columns {
            c.sheet_reg_id = sheet_reg_id.to_string();
            let k = column_key(sheet_reg_id, &c.header);
            if !out.iter().any(|(_, x)| x == &k) {
                out.push((Table::Columns, k.clone()));
            }
            self.columns.insert(k, c);
        }
        out
    }

    pub fn notified(&self, sheet_reg_id: &str) -> Vec<NotifiedRow> {
        let mut out: Vec<_> = self
            .notified
            .values()
            .filter(|n| n.sheet_reg_id == sheet_reg_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| (a.at, &a.fingerprint).cmp(&(b.at, &b.fingerprint)));
        out
    }
    pub fn put_notified(&mut self, rows: Vec<NotifiedRow>) -> Changed {
        let mut out = Vec::new();
        for r in rows {
            let k = column_key(&r.sheet_reg_id, &r.fingerprint);
            if r.sheet_reg_id.is_empty() || r.fingerprint.is_empty() {
                continue;
            }
            match self.notified.get(&k) {
                Some(old) if old.state == NotifyState::Sent => continue,
                Some(old) if *old == r => continue,
                _ => {}
            }
            self.notified.insert(k.clone(), r);
            out.push((Table::Notified, k));
        }
        out
    }
    pub fn purge_notified(&mut self, before: DateTime<Utc>) -> Changed {
        let old: Vec<String> = self
            .notified
            .iter()
            .filter(|(_, n)| n.at < before)
            .map(|(k, _)| k.clone())
            .collect();
        for k in &old {
            self.notified.remove(k);
        }
        old.into_iter().map(|k| (Table::Notified, k)).collect()
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        if self.logs.len() >= RECENT_LOGS {
            self.logs.pop_front();
        }
        self.logs.push_back(entry);
    }
    pub fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        self.logs.iter().rev().take(limit).cloned().collect()
    }
}

/// プロセス内のメモリだけに置く保存先 (テストと手元の確認用。再起動で消える)。
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Tables>,
}

impl MemoryStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, Tables> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl PortalStore for MemoryStore {
    fn accounts(&self) -> Vec<Account> {
        self.lock().accounts()
    }
    fn account(&self, id: &str) -> Option<Account> {
        self.lock().accounts.get(id).cloned()
    }
    fn account_by_email(&self, email: &str) -> Option<Account> {
        self.lock().account_by_email(email)
    }
    fn insert_account(&self, account: Account) -> Result<(), StoreError> {
        self.lock().insert_account(account).map(drop)
    }
    fn update_account(&self, account: Account) -> Result<(), StoreError> {
        self.lock().update_account(account).map(drop)
    }
    fn touch_login(&self, account_id: &str, now: DateTime<Utc>) {
        self.lock().touch_login(account_id, now);
    }

    fn insert_invite(&self, invite: Invite) {
        self.lock().insert_invite(invite);
    }
    fn invite(&self, token_hash: &str) -> Option<Invite> {
        self.lock().invites.get(token_hash).cloned()
    }
    fn use_invite(&self, token_hash: &str, now: DateTime<Utc>) -> bool {
        self.lock().use_invite(token_hash, now).is_some()
    }
    fn revoke_invites(&self, account_id: &str) {
        self.lock().revoke_invites(account_id);
    }

    fn insert_device(&self, device: Device) {
        self.lock().insert_device(device);
    }
    fn device(&self, token_hash: &str) -> Option<Device> {
        self.lock().devices.get(token_hash).cloned()
    }
    fn touch_device(&self, token_hash: &str, now: DateTime<Utc>) {
        self.lock().touch_device(token_hash, now);
    }
    fn revoke_devices(&self, account_id: &str) {
        self.lock().revoke_devices(account_id);
    }

    fn insert_session(&self, session: Session) {
        self.lock().insert_session(session);
    }
    fn session(&self, token_hash: &str) -> Option<Session> {
        self.lock().sessions.get(token_hash).cloned()
    }
    fn end_session(&self, token_hash: &str, now: DateTime<Utc>) {
        self.lock().end_session(token_hash, now);
    }
    fn end_sessions(&self, account_id: &str, now: DateTime<Utc>) {
        self.lock().end_sessions(account_id, now);
    }

    fn customers(&self) -> Vec<Customer> {
        self.lock().customers()
    }
    fn customer(&self, id: &str) -> Option<Customer> {
        self.lock().customers.get(id).cloned()
    }
    fn insert_customer(&self, customer: Customer) -> Result<(), StoreError> {
        self.lock().insert_customer(customer).map(drop)
    }
    fn update_customer(&self, customer: Customer) -> Result<(), StoreError> {
        self.lock().update_customer(customer).map(drop)
    }

    fn sheets(&self, customer_id: Option<&str>) -> Vec<SheetReg> {
        self.lock().sheets(customer_id)
    }
    fn sheet(&self, id: &str) -> Option<SheetReg> {
        self.lock().sheets.get(id).cloned()
    }
    fn insert_sheet(&self, sheet: SheetReg) -> Result<(), StoreError> {
        self.lock().insert_sheet(sheet).map(drop)
    }
    fn update_sheet(&self, sheet: SheetReg) -> Result<(), StoreError> {
        self.lock().update_sheet(sheet).map(drop)
    }

    fn columns(&self, sheet_reg_id: &str) -> Vec<ColumnSetting> {
        self.lock().columns(sheet_reg_id)
    }
    fn replace_columns(&self, sheet_reg_id: &str, columns: Vec<ColumnSetting>) {
        self.lock().replace_columns(sheet_reg_id, columns);
    }

    fn notified(&self, sheet_reg_id: &str) -> Vec<NotifiedRow> {
        self.lock().notified(sheet_reg_id)
    }
    fn put_notified(&self, rows: Vec<NotifiedRow>) {
        self.lock().put_notified(rows);
    }
    fn purge_notified(&self, before: DateTime<Utc>) -> usize {
        self.lock().purge_notified(before).len()
    }

    fn append_log(&self, entry: LogEntry) {
        self.lock().push_log(entry);
    }
    fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        self.lock().recent_logs(limit)
    }

    fn status(&self) -> StoreStatus {
        StoreStatus {
            kind: "memory",
            ready: true,
            pending: 0,
            last_error: None,
            loaded_at: None,
            last_saved_at: None,
        }
    }
}
