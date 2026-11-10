//! 新規営業のアポを Slack Bot (`chat.postMessage`) でチームのチャンネルへ送る (2026-11-10 ユーザー決定)。
//!
//! BPO の Incoming Webhook (`slack.rs`) とは別の経路。Webhook では投稿の ts が取れずスレッドに書けないため、
//! 新規営業は Bot で親の投稿 (営業本人とリーダーへのメンション + アポのランク) を出し、
//! その ts をスレッドにして `slack::message` の中身 (担当者・時刻・ランク・ヒアリング・リンク) を返信する。
//!
//! 宛先はスプレッドシート「新規営業組織指定（Slack通知設定）」から引く (サービスアカウントで読み取りのみ。10 分キャッシュ)。
//! - ①チーム設定_入力: チーム名 / Slack user ID (リーダー) / 有効 / アポ通知先チャンネルID
//!   (同じタブの「Slack通知先チャンネルID」は別の仕組みが使うので読まない)
//! - ②メンバー配置_入力: 所属チームを選択 / HubSpot ID / 有効
//! - 社員マスタ_API: HubSpot owner ID / Slack user ID
//!
//! 環境変数:
//! - `TALK_REVIEW_SLACK_BOT_TOKEN`: Bot のトークン (`xoxb-`、秘密。ログに出さない)。無ければこの経路は丸ごと無効
//! - `TALK_REVIEW_SLACK_ROUTING_SHEET_ID`: 上のスプレッドシートの ID。無ければ無効
//! - `GOOGLE_SA_KEY_B64`: シートを読むサービスアカウント (既存)

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::json;

use super::apo_rank;
use super::config::TalkReviewConfig;
use super::slack::{app_url_from, message, ApoNotice, MAX_AGE};

const ATTEMPTS: u32 = 3;
const ROUTING_TTL: Duration = Duration::from_secs(600);
/// 読めなかったときに次に読み直すまでの間 (前回の結果を使い続ける)
const ROUTING_RETRY: Duration = Duration::from_secs(60);
const SLACK_API: &str = "https://slack.com/api";

pub const TAB_TEAMS: &str = "①チーム設定_入力";
pub const TAB_MEMBERS: &str = "②メンバー配置_入力";
pub const TAB_STAFF: &str = "社員マスタ_API";

/// シートの 1 タブ (見出し, 行)
pub type Table = (Vec<String>, Vec<Vec<String>>);
type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// タブを読む口 (本番は `SheetsClient`、テストは固定の表)
pub trait SheetSource: Send + Sync {
    fn table<'a>(&'a self, tab: &'a str) -> BoxFut<'a, anyhow::Result<Table>>;
}

impl SheetSource for crate::db::sheets_client::SheetsClient {
    fn table<'a>(&'a self, tab: &'a str) -> BoxFut<'a, anyhow::Result<Table>> {
        Box::pin(self.get_sheet_as_table(tab))
    }
}

// ---- シート → 宛先 ------------------------------------------------------

/// 見出しを比べる形にする: 空白・改行を除き、小文字にし、括弧 (「（自動）」等) から後ろを落とす
fn norm_header(h: &str) -> String {
    let s: String = h
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect();
    match s.find(['（', '(']) {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// 見出しの列を探す。整えた形が一致する列を先に、無ければ先頭一致
fn find_col(header: &[String], key: &str) -> Option<usize> {
    let key = norm_header(key);
    let normed: Vec<String> = header.iter().map(|h| norm_header(h)).collect();
    normed
        .iter()
        .position(|h| *h == key)
        .or_else(|| normed.iter().position(|h| h.starts_with(&key)))
}

fn cell(row: &[String], col: Option<usize>) -> String {
    let v = col.and_then(|i| row.get(i)).map(|s| s.trim()).unwrap_or("");
    // 数値で返った ID が "123.0" になっても同じ ID として扱う
    v.strip_suffix(".0").unwrap_or(v).to_string()
}

fn enabled(row: &[String], col: Option<usize>) -> bool {
    cell(row, col).eq_ignore_ascii_case("true")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TeamInfo {
    leader_slack: Option<String>,
    channel: Option<String>,
}

/// シートから作った宛先の表
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Routing {
    /// 有効なチームだけ
    teams: HashMap<String, TeamInfo>,
    /// HubSpot owner ID → チーム名 (有効なメンバーだけ)
    owner_team: HashMap<String, String>,
    /// HubSpot owner ID → Slack user ID
    owner_slack: HashMap<String, String>,
}

/// 1 件の送り先
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub channel: String,
    /// 親の投稿でメンションする人 (営業本人, リーダーの順。重複なし)
    pub mentions: Vec<String>,
}

impl Routing {
    pub fn parse(teams: &Table, members: &Table, staff: &Table) -> Self {
        let mut r = Routing::default();
        let (h, rows) = teams;
        let (name, leader, on, channel) = (
            find_col(h, "チーム名"),
            find_col(h, "Slack user ID"),
            find_col(h, "有効"),
            find_col(h, "アポ通知先チャンネルID"),
        );
        for row in rows {
            let team = cell(row, name);
            if team.is_empty() || !enabled(row, on) {
                continue;
            }
            let nonempty = |s: String| (!s.is_empty()).then_some(s);
            r.teams.entry(team).or_insert(TeamInfo {
                leader_slack: nonempty(cell(row, leader)),
                channel: nonempty(cell(row, channel)),
            });
        }
        let (h, rows) = members;
        let (team, owner, on) = (
            find_col(h, "所属チームを選択"),
            find_col(h, "HubSpot ID"),
            find_col(h, "有効"),
        );
        for row in rows {
            let (t, o) = (cell(row, team), cell(row, owner));
            if t.is_empty() || o.is_empty() || !enabled(row, on) {
                continue;
            }
            r.owner_team.entry(o).or_insert(t);
        }
        let (h, rows) = staff;
        let (owner, slack) = (
            find_col(h, "HubSpot owner ID"),
            find_col(h, "Slack user ID"),
        );
        for row in rows {
            let (o, s) = (cell(row, owner), cell(row, slack));
            if !o.is_empty() && !s.is_empty() {
                r.owner_slack.entry(o).or_insert(s);
            }
        }
        r
    }

    /// 担当者の送り先。送れないときは理由 (ログ用。個人名は含めない)
    pub fn resolve(&self, owner_id: &str) -> Result<Destination, &'static str> {
        let team = self
            .owner_team
            .get(owner_id)
            .ok_or("担当者がメンバー配置に無い・無効")?;
        let info = self.teams.get(team).ok_or("チームが無い・無効")?;
        let channel = info
            .channel
            .clone()
            .ok_or("チームのアポ通知先チャンネルIDが空")?;
        let mut mentions = Vec::new();
        for id in [self.owner_slack.get(owner_id), info.leader_slack.as_ref()]
            .into_iter()
            .flatten()
        {
            if !mentions.contains(id) {
                mentions.push(id.clone());
            }
        }
        Ok(Destination { channel, mentions })
    }
}

/// 親の投稿の文
pub fn parent_text(mentions: &[String], rank: &str) -> String {
    let mut s: String = mentions.iter().map(|m| format!("<@{m}> ")).collect();
    s.push_str(&format!("アポ ランク {rank}"));
    s
}

// ---- シートのキャッシュ ----------------------------------------------------

struct CacheState {
    routing: Option<Arc<Routing>>,
    next_load: Option<Instant>,
}

/// 10 分キャッシュ。読めなかったら前回の結果を使い、一度も読めていなければ `None`
pub struct RoutingCache {
    source: Arc<dyn SheetSource>,
    state: tokio::sync::Mutex<CacheState>,
}

impl RoutingCache {
    pub fn new(source: Arc<dyn SheetSource>) -> Self {
        Self {
            source,
            state: tokio::sync::Mutex::new(CacheState {
                routing: None,
                next_load: None,
            }),
        }
    }

    async fn load(&self) -> anyhow::Result<Routing> {
        let teams = self.source.table(TAB_TEAMS).await?;
        let members = self.source.table(TAB_MEMBERS).await?;
        let staff = self.source.table(TAB_STAFF).await?;
        Ok(Routing::parse(&teams, &members, &staff))
    }

    pub async fn get(&self) -> Option<Arc<Routing>> {
        let mut st = self.state.lock().await;
        let now = Instant::now();
        if st.next_load.is_some_and(|t| now < t) {
            return st.routing.clone();
        }
        match self.load().await {
            Ok(r) => {
                st.routing = Some(Arc::new(r));
                st.next_load = Some(now + ROUTING_TTL);
            }
            Err(e) => {
                tracing::warn!(error = %e, has_previous = st.routing.is_some(), "talk review: slack routing sheet read failed");
                st.next_load = Some(now + ROUTING_RETRY);
            }
        }
        st.routing.clone()
    }
}

// ---- Slack Bot ----------------------------------------------------------

#[derive(Clone)]
pub struct BotNotifier {
    inner: Arc<Inner>,
}

struct Inner {
    token: String,
    api_base: String,
    app_url: String,
    routing: RoutingCache,
    http: reqwest::Client,
    retry_wait: Duration,
}

impl BotNotifier {
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(
            |k| std::env::var(k).ok(),
            |id| {
                crate::db::sheets_client::SheetsClient::from_env_for(id)
                    .map(|c| Arc::new(c) as Arc<dyn SheetSource>)
            },
        )
    }

    pub fn from_lookup(
        get: impl Fn(&str) -> Option<String>,
        sheets: impl FnOnce(String) -> anyhow::Result<Arc<dyn SheetSource>>,
    ) -> Option<Self> {
        let text = |k: &str| {
            get(k)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let token = text("TALK_REVIEW_SLACK_BOT_TOKEN")?;
        if !token.starts_with("xoxb-") {
            tracing::warn!("talk review: TALK_REVIEW_SLACK_BOT_TOKEN が Bot のトークンの形でないため新規営業の Slack 通知を止めます");
            return None;
        }
        let Some(sheet_id) = text("TALK_REVIEW_SLACK_ROUTING_SHEET_ID") else {
            tracing::warn!("talk review: TALK_REVIEW_SLACK_ROUTING_SHEET_ID が無いため新規営業の Slack 通知を止めます");
            return None;
        };
        let source = match sheets(sheet_id) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "talk review: 宛先のシートを読めないため新規営業の Slack 通知を止めます");
                return None;
            }
        };
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;
        Some(Self::build(
            token,
            SLACK_API.into(),
            app_url_from(&get),
            source,
            http,
            Duration::from_secs(2),
        ))
    }

    fn build(
        token: String,
        api_base: String,
        app_url: String,
        source: Arc<dyn SheetSource>,
        http: reqwest::Client,
        retry_wait: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                token,
                api_base,
                app_url,
                routing: RoutingCache::new(source),
                http,
                retry_wait,
            }),
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(api_base: String, source: Arc<dyn SheetSource>) -> Self {
        Self::build(
            "xoxb-test".into(),
            api_base,
            "https://example.test".into(),
            source,
            reqwest::Client::new(),
            Duration::from_millis(1),
        )
    }

    /// このチームの電話を Bot で送るか (BPO は Webhook の経路のまま)
    pub fn covers(team: &str) -> bool {
        !TalkReviewConfig::is_bpo_team(team)
    }

    /// 1 回投稿する。成功なら ts
    async fn post_once(&self, body: &serde_json::Value) -> Result<String, String> {
        let i = &self.inner;
        let r = i
            .http
            .post(format!("{}/chat.postMessage", i.api_base))
            .bearer_auth(&i.token)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("send failed (timeout={})", e.is_timeout()))?;
        let status = r.status();
        if !status.is_success() {
            return Err(format!("http {}", status.as_u16()));
        }
        let v: serde_json::Value = r.json().await.map_err(|_| "unreadable response")?;
        if v.get("ok").and_then(|b| b.as_bool()) != Some(true) {
            let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown");
            return Err(format!("slack error: {err}"));
        }
        v.get("ts")
            .and_then(|t| t.as_str())
            .map(str::to_string)
            .ok_or_else(|| "no ts".to_string())
    }

    /// `ATTEMPTS` 回まで投稿する
    async fn post(&self, body: serde_json::Value, what: &'static str) -> Option<String> {
        for attempt in 1..=ATTEMPTS {
            match self.post_once(&body).await {
                Ok(ts) => return Some(ts),
                Err(e) => {
                    tracing::warn!(error = %e, attempt, what, "talk review: slack bot post failed")
                }
            }
            if attempt < ATTEMPTS {
                tokio::time::sleep(self.inner.retry_wait * attempt).await;
            }
        }
        None
    }

    /// 送る。親の投稿が出せたら true (スレッドの返信は失敗しても親を出し直さない)
    pub async fn send(&self, n: &ApoNotice<'_>, now: DateTime<Utc>) -> bool {
        if !Self::covers(n.team) || now - n.call_at > MAX_AGE {
            return false;
        }
        let (Some(rank), Some(detail)) = (apo_rank::rank(n.items), message(n, &self.inner.app_url))
        else {
            return false;
        };
        let Some(routing) = self.inner.routing.get().await else {
            tracing::warn!("talk review: slack routing sheet has never been read; not posting");
            return false;
        };
        let dest = match routing.resolve(n.owner_id) {
            Ok(d) => d,
            Err(reason) => {
                tracing::info!(reason, "talk review: sales apo not posted to slack");
                return false;
            }
        };
        let parent = json!({
            "channel": dest.channel,
            "text": parent_text(&dest.mentions, rank),
        });
        let Some(ts) = self.post(parent, "parent").await else {
            return false;
        };
        let reply = json!({
            "channel": dest.channel,
            "thread_ts": ts,
            "text": detail,
            "unfurl_links": false,
            "unfurl_media": false,
        });
        if self.post(reply, "thread").await.is_none() {
            tracing::warn!("talk review: slack thread reply failed; parent kept");
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::judge::StoredItem;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn t(header: &[&str], rows: &[&[&str]]) -> Table {
        (
            header.iter().map(|s| s.to_string()).collect(),
            rows.iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        )
    }

    /// 架空の値だけ。見出しは実物と同じゆれ (改行・括弧・列の順番の違い) を持たせる
    fn teams() -> Table {
        t(
            &[
                "チーム名",
                "Slack user ID\n（自動）",
                "リーダー名",
                "有効",
                "Slack通知先チャンネルID",
                "アポ通知先チャンネルID\n（入力する）",
            ],
            &[
                &["東", "ULEAD1", "x", "TRUE", "CWRONG1", "CAPO1"],
                &["西", "ULEAD2", "x", "true", "CWRONG2", ""],
                &["南", "ULEAD3", "x", "FALSE", "CWRONG3", "CAPO3"],
                &["北", "ULEAD4", "x", "", "CWRONG4", "CAPO4"],
                &["中", "", "x", "TRUE", "CWRONG5", "CAPO5"],
            ],
        )
    }

    fn members() -> Table {
        t(
            &["名前", "所属チームを選択", "有効", "HubSpot ID（自動）"],
            &[
                &["a", "東", "TRUE", "101"],
                &["b", "西", "TRUE", "102"],
                &["c", "南", "TRUE", "103"],
                &["d", "東", "FALSE", "104"],
                &["e", "東", "TRUE", "105.0"],
                &["f", "中", "TRUE", "106"],
                &["g", "北", "TRUE", "107"],
            ],
        )
    }

    fn staff() -> Table {
        t(
            &["氏名", "HubSpot owner ID", "Slack user ID"],
            &[
                &["a", "101", "USALES1"],
                &["lead", "999", "ULEAD1"],
                &["e", "105", "ULEAD1"],
            ],
        )
    }

    fn routing() -> Routing {
        Routing::parse(&teams(), &members(), &staff())
    }

    #[test]
    fn header_matching_tolerates_newlines_and_suffixes() {
        let h: Vec<String> = [
            "Slack通知先チャンネルID",
            "アポ通知先\nチャンネルID （入力する）",
            "HubSpot owner ID",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(find_col(&h, "アポ通知先チャンネルID"), Some(1));
        assert_eq!(find_col(&h, "Slack通知先チャンネルID"), Some(0));
        assert_eq!(find_col(&h, "hubspot owner id"), Some(2));
        assert_eq!(find_col(&h, "チーム名"), None);
    }

    #[test]
    fn resolves_sales_rep_and_leader_from_sheet_rows() {
        let r = routing();
        assert_eq!(
            r.resolve("101"),
            Ok(Destination {
                channel: "CAPO1".into(),
                mentions: vec!["USALES1".into(), "ULEAD1".into()],
            })
        );
        // 本人 = リーダーなら 1 回だけ ("105.0" も 105 として読む)
        assert_eq!(r.resolve("105").unwrap().mentions, vec!["ULEAD1"]);
        // 本人もリーダーも Slack ID が無ければメンションなしで送る
        assert_eq!(r.resolve("106").unwrap().mentions, Vec::<String>::new());
        // 送らない: チャンネル空 / チーム無効 (FALSE・空) / メンバー無効 / 載っていない
        assert_eq!(r.resolve("102"), Err("チームのアポ通知先チャンネルIDが空"));
        assert_eq!(r.resolve("103"), Err("チームが無い・無効"));
        assert_eq!(r.resolve("107"), Err("チームが無い・無効"));
        assert_eq!(r.resolve("104"), Err("担当者がメンバー配置に無い・無効"));
        assert_eq!(r.resolve("999"), Err("担当者がメンバー配置に無い・無効"));
    }

    #[test]
    fn leader_only_when_rep_has_no_slack_id() {
        let staff = t(&["HubSpot owner ID", "Slack user ID"], &[]);
        let r = Routing::parse(&teams(), &members(), &staff);
        assert_eq!(r.resolve("101").unwrap().mentions, vec!["ULEAD1"]);
    }

    #[test]
    fn no_apo_channel_column_means_nobody_is_posted() {
        let teams = t(
            &[
                "チーム名",
                "Slack user ID（自動）",
                "有効",
                "Slack通知先チャンネルID",
            ],
            &[&["東", "ULEAD1", "TRUE", "CWRONG1"]],
        );
        let r = Routing::parse(&teams, &members(), &staff());
        assert_eq!(r.resolve("101"), Err("チームのアポ通知先チャンネルIDが空"));
    }

    #[test]
    fn parent_text_mentions_then_rank() {
        assert_eq!(
            parent_text(&["U1".into(), "U2".into()], "A"),
            "<@U1> <@U2> アポ ランク A"
        );
        assert_eq!(parent_text(&[], "B"), "アポ ランク B");
    }

    #[test]
    fn config_needs_bot_token_and_sheet_id() {
        let src = || Ok(Arc::new(Fixed::ok()) as Arc<dyn SheetSource>);
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(kk, _)| *kk == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert!(BotNotifier::from_lookup(|_| None, |_| src()).is_none());
        assert!(BotNotifier::from_lookup(
            env(&[("TALK_REVIEW_SLACK_BOT_TOKEN", "xoxb-1")]),
            |_| src()
        )
        .is_none());
        assert!(BotNotifier::from_lookup(
            env(&[
                ("TALK_REVIEW_SLACK_BOT_TOKEN", "xoxp-1"),
                ("TALK_REVIEW_SLACK_ROUTING_SHEET_ID", "sheet")
            ]),
            |_| src()
        )
        .is_none());
        assert!(BotNotifier::from_lookup(
            env(&[
                ("TALK_REVIEW_SLACK_BOT_TOKEN", "xoxb-1"),
                ("TALK_REVIEW_SLACK_ROUTING_SHEET_ID", "sheet")
            ]),
            |_| Err(anyhow::anyhow!("no key"))
        )
        .is_none());
        let seen = Mutex::new(String::new());
        assert!(BotNotifier::from_lookup(
            env(&[
                ("TALK_REVIEW_SLACK_BOT_TOKEN", "xoxb-1"),
                ("TALK_REVIEW_SLACK_ROUTING_SHEET_ID", " sheet-x ")
            ]),
            |id| {
                *seen.lock().unwrap() = id;
                src()
            }
        )
        .is_some());
        assert_eq!(*seen.lock().unwrap(), "sheet-x");
        assert!(BotNotifier::covers("新規営業"));
        assert!(!BotNotifier::covers("BPO_リクロジ"));
    }

    // ---- キャッシュと Slack のモック ----

    struct Fixed {
        fail: std::sync::atomic::AtomicBool,
        reads: AtomicUsize,
    }

    impl Fixed {
        fn ok() -> Self {
            Self {
                fail: false.into(),
                reads: AtomicUsize::new(0),
            }
        }
    }

    impl SheetSource for Fixed {
        fn table<'a>(&'a self, tab: &'a str) -> BoxFut<'a, anyhow::Result<Table>> {
            Box::pin(async move {
                self.reads.fetch_add(1, Ordering::SeqCst);
                if self.fail.load(Ordering::SeqCst) {
                    anyhow::bail!("sheet down");
                }
                Ok(match tab {
                    TAB_TEAMS => teams(),
                    TAB_MEMBERS => members(),
                    TAB_STAFF => staff(),
                    _ => anyhow::bail!("unknown tab"),
                })
            })
        }
    }

    #[tokio::test]
    async fn cache_keeps_previous_routing_when_sheet_fails() {
        let src = Arc::new(Fixed::ok());
        src.fail.store(true, Ordering::SeqCst);
        let cache = RoutingCache::new(src.clone());
        assert!(
            cache.get().await.is_none(),
            "一度も読めていなければ送らない"
        );
        // 失敗後は 1 分待つ。待ちを飛ばして読み直す
        cache.state.lock().await.next_load = None;
        src.fail.store(false, Ordering::SeqCst);
        let r = cache.get().await.unwrap();
        assert_eq!(r.resolve("101").unwrap().channel, "CAPO1");
        let reads = src.reads.load(Ordering::SeqCst);
        // 10 分以内は読まない
        cache.get().await.unwrap();
        assert_eq!(src.reads.load(Ordering::SeqCst), reads);
        // 期限切れで読めなくても前回の結果を使う
        cache.state.lock().await.next_load = None;
        src.fail.store(true, Ordering::SeqCst);
        assert_eq!(cache.get().await.unwrap(), r);
    }

    /// Slack の chat.postMessage のモック。`replies` は呼ばれた順に返す本文 (尽きたら最後を繰り返す)
    async fn mock_slack(
        replies: Vec<serde_json::Value>,
    ) -> (
        String,
        Arc<Mutex<Vec<serde_json::Value>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (seen2, n) = (seen.clone(), Arc::new(AtomicUsize::new(0)));
        let replies = Arc::new(replies);
        let app = axum::Router::new().route(
            "/api/chat.postMessage",
            axum::routing::post(
                move |headers: axum::http::HeaderMap,
                      axum::Json(body): axum::Json<serde_json::Value>| {
                    let (seen, n, replies) = (seen2.clone(), n.clone(), replies.clone());
                    async move {
                        assert_eq!(headers.get("authorization").unwrap(), "Bearer xoxb-test");
                        seen.lock().unwrap().push(body);
                        let i = n.fetch_add(1, Ordering::SeqCst).min(replies.len() - 1);
                        axum::Json(replies[i].clone())
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (base, seen, server)
    }

    fn item(id: &str, answer: &str) -> StoredItem {
        StoredItem {
            id: id.into(),
            status: "done".into(),
            evidence: vec![],
            quote: Some("引用は送らない".into()),
            advice: None,
            note: None,
            answer: Some(answer.into()),
            asked: Some(true),
            answered: Some(true),
            counted_from: None,
        }
    }

    fn items() -> Vec<StoredItem> {
        vec![
            item("aq_budget", "具体額"),
            item("aq_timing", "3か月以内"),
            item("aq_job", "現場系"),
            item("aq_reason", "欠員"),
            item("aq_decider", "本人が判断者"),
        ]
    }

    fn notice<'a>(owner: &'a str, team: &'a str, items: &'a [StoredItem]) -> ApoNotice<'a> {
        ApoNotice {
            call_id: "555",
            owner_id: owner,
            owner_name: "架空の営業",
            team,
            call_at: "2026-11-05T01:30:00Z".parse().unwrap(),
            items,
        }
    }

    fn now() -> DateTime<Utc> {
        "2026-11-05T02:00:00Z".parse().unwrap()
    }

    #[tokio::test]
    async fn posts_parent_then_thread_reply_with_details() {
        let (base, seen, server) = mock_slack(vec![json!({"ok": true, "ts": "111.222"})]).await;
        let bot = BotNotifier::for_test(base, Arc::new(Fixed::ok()));
        let items = items();
        assert!(bot.send(&notice("101", "新規営業", &items), now()).await);
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0]["channel"], "CAPO1");
        assert_eq!(seen[0]["text"], "<@USALES1> <@ULEAD1> アポ ランク S");
        assert!(seen[0].get("thread_ts").is_none());
        assert_eq!(seen[1]["channel"], "CAPO1");
        assert_eq!(seen[1]["thread_ts"], "111.222");
        let text = seen[1]["text"].as_str().unwrap();
        assert!(
            text.starts_with("*アポ ランク S*　架空の営業（新規営業）11/05 10:30\n"),
            "{text}"
        );
        assert!(text.contains("予算: 具体額 / 入社時期: 3か月以内"));
        assert!(text.contains("owner=101&call=555|振り返りで開く>"));
        assert!(!text.contains("引用は送らない"));
        server.abort();
    }

    #[tokio::test]
    async fn does_not_post_for_bpo_unrouted_old_or_unranked_calls() {
        let (base, seen, server) = mock_slack(vec![json!({"ok": true, "ts": "1.1"})]).await;
        let bot = BotNotifier::for_test(base, Arc::new(Fixed::ok()));
        let items = items();
        assert!(
            !bot.send(&notice("101", "BPO_リクロジ", &items), now())
                .await
        );
        assert!(!bot.send(&notice("102", "新規営業", &items), now()).await);
        assert!(!bot.send(&notice("104", "新規営業", &items), now()).await);
        assert!(!bot.send(&notice("101", "新規営業", &[]), now()).await);
        let late = now() + MAX_AGE;
        assert!(!bot.send(&notice("101", "新規営業", &items), late).await);
        assert!(seen.lock().unwrap().is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn ok_false_is_an_error_and_parent_is_retried_three_times() {
        let (base, seen, server) =
            mock_slack(vec![json!({"ok": false, "error": "channel_not_found"})]).await;
        let bot = BotNotifier::for_test(base, Arc::new(Fixed::ok()));
        let items = items();
        assert!(!bot.send(&notice("101", "新規営業", &items), now()).await);
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 3, "親を 3 回試して、スレッドは送らない");
        assert!(seen.iter().all(|b| b.get("thread_ts").is_none()));
        server.abort();
    }

    #[tokio::test]
    async fn failing_thread_reply_is_retried_without_reposting_parent() {
        let (base, seen, server) = mock_slack(vec![
            json!({"ok": false, "error": "ratelimited"}),
            json!({"ok": true, "ts": "9.9"}),
            json!({"ok": false, "error": "internal_error"}),
        ])
        .await;
        let bot = BotNotifier::for_test(base, Arc::new(Fixed::ok()));
        let items = items();
        assert!(bot.send(&notice("101", "新規営業", &items), now()).await);
        let seen = seen.lock().unwrap().clone();
        // 親: 失敗 1 + 成功 1、返信: 3 回とも失敗。親は出し直さない
        assert_eq!(seen.len(), 5);
        let parents = seen.iter().filter(|b| b.get("thread_ts").is_none()).count();
        assert_eq!(parents, 2);
        assert!(seen[2..].iter().all(|b| b["thread_ts"] == "9.9"));
        server.abort();
    }
}
