//! HubSpot 呼び出しの関所 (gateway)。プロセス内の全 HubSpot 呼び出しが 1 つの流量制限を共有する。
//!
//! 設計: `docs/architecture/headless-crm-design.md` §18 (HubSpot 呼び出しの関所)。
//!
//! - **流量制限 (lane)**: 通常の呼び出し (`General`) は「1 秒あたり」「10 秒あたり」の 2 つの窓で数える。
//!   Search (`Search`) は別の窓 (既定 1 秒に 3 回 = 333ms 間隔)。窓は直近の許可時刻の記録 (sliding window) で判定するので、
//!   どの 1 秒・どの 10 秒を切り取っても設定値を超えない。鍵は既存の外部バッチと共有するため、既定値は
//!   HubSpot の上限 (190 / 10 秒、19 / 秒) より十分低くしてある。
//! - **優先度**: 待っている要求は優先度ごとの列に並び、`Interactive` (画面の操作) の列が空のときだけ
//!   `Background` (キャッシュの更新・常駐の取得) が進む。同じ優先度の中は到着順。
//! - **待ちの上限**: 優先度ごとに待てる時間がある。並んだ時点の見積もりが上限を超えるなら並ばずに、
//!   並んだ後でも上限に達したら列から抜けて [`Busy`] を返す (呼び出し側は 503 `hubspot_busy`)。
//! - **429**: どの要求が 429 を受けても、`Retry-After` (無ければ最低待ち時間) の間は全員を止める。
//! - **観測**: 呼び出し回数 (種類別)・Search 回数・429 回数・まとめた回数・断った回数・待ち時間 (直近 5 分の
//!   p50 / p95)・最後に見た `X-HubSpot-RateLimit-*` を持つ。`/api/admin/hubspot-usage` が読む。鍵は持たない。
//!
//! 時刻は `tokio::time::Instant` (テストで `start_paused` の仮想時計を使えるように)。

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use reqwest::header::HeaderMap;
use tokio::sync::Notify;
use tokio::time::Instant;

/// 呼び出しの優先度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Priority {
    /// 画面の操作で人が待っている読み取り
    Interactive,
    /// キャッシュの先読み・定期の取得など、人が直接待っていないもの
    Background,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Interactive => "interactive",
            Priority::Background => "background",
        }
    }
}

/// 流量制限の窓の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// Search 以外の全部
    General,
    /// `POST /crm/v3/objects/{object}/search` (HubSpot 側の上限も別)
    Search,
}

impl Lane {
    fn idx(self) -> usize {
        match self {
            Lane::General => 0,
            Lane::Search => 1,
        }
    }
}

/// 待ちの上限を超える (または超えると見込まれる) ので断った
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Busy;

/// 関所の設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayConfig {
    /// Search 以外の 1 秒あたりの上限 (0 = 制限なし)
    pub per_second: u32,
    /// Search 以外の 10 秒あたりの上限 (0 = 制限なし)
    pub per_10s: u32,
    /// Search の開始間隔 (ZERO = 制限なし)
    pub search_interval: Duration,
    /// 画面の操作 (`Interactive`) が待てる上限
    pub interactive_max_wait: Duration,
    /// 背景の取得 (`Background`) が待てる上限
    pub background_max_wait: Duration,
    /// 429 で全員を止める最短の時間 (`Retry-After` が 0 や無いときも)
    pub rate_limited_min_pause: Duration,
    /// 429 で全員を止める最長の時間 (`Retry-After` が大きすぎるとき)
    pub max_pause: Duration,
}

/// 既定値 (環境変数で上書きできる。`docs/env_variables_reference.md`)
pub const DEFAULT_PER_SECOND: u32 = 8;
pub const DEFAULT_PER_10S: u32 = 80;
/// Search はアカウントで約 5 回/秒。外部バッチの分を残して 3 回/秒 (2026-10-08 負荷試験: 1 回/秒では
/// 100 人の朝の架電キューで先頭ページの 9 割が 20 秒の締め切りに間に合わなかった)
pub const DEFAULT_SEARCH_PER_SECOND: u32 = 3;
pub const DEFAULT_INTERACTIVE_MAX_WAIT_MS: u64 = 5_000;
pub const DEFAULT_BACKGROUND_MAX_WAIT_MS: u64 = 60_000;
/// 429 の停止の上限 (`client.rs` の retry の待ちの上限と同じ)
pub const MAX_PAUSE: Duration = Duration::from_secs(10);

/// 待ち時間の記録を残す期間 (p50 / p95 の対象)
pub const WAIT_WINDOW: Duration = Duration::from_secs(5 * 60);
/// 待ち時間の記録の上限件数 (古いものから捨てる)
const WAIT_SAMPLES_MAX: usize = 20_000;

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            per_second: DEFAULT_PER_SECOND,
            per_10s: DEFAULT_PER_10S,
            search_interval: Duration::from_millis(1000 / u64::from(DEFAULT_SEARCH_PER_SECOND)),
            interactive_max_wait: Duration::from_millis(DEFAULT_INTERACTIVE_MAX_WAIT_MS),
            background_max_wait: Duration::from_millis(DEFAULT_BACKGROUND_MAX_WAIT_MS),
            rate_limited_min_pause: Duration::from_secs(1),
            max_pause: MAX_PAUSE,
        }
    }
}

/// 環境変数の値 `raw` が正の整数として読めて範囲内ならその値、そうでなければ既定値 (不正な値は warn)。
/// 名前は `env::var("...")` の形で呼び出し側に書く (`tests/env_example_matches_code.rs` が拾えるように)
fn env_u64(name: &str, raw: Option<String>, default: u64, min: u64, max: u64) -> u64 {
    match raw {
        None => default,
        Some(raw) if raw.trim().is_empty() => default,
        Some(raw) => match raw.trim().parse::<u64>() {
            Ok(v) if (min..=max).contains(&v) => v,
            _ => {
                tracing::warn!(
                    name,
                    min,
                    max,
                    default,
                    "HubSpot の流量設定の値が範囲外か数字でないため既定値を使います"
                );
                default
            }
        },
    }
}

impl GatewayConfig {
    /// 環境変数から読む (未設定・不正は既定値)
    pub fn from_env() -> Self {
        let per_second = env_u64(
            "HUBSPOT_APP_RATE_PER_SEC",
            std::env::var("HUBSPOT_APP_RATE_PER_SEC").ok(),
            u64::from(DEFAULT_PER_SECOND),
            1,
            19,
        ) as u32;
        let per_10s = env_u64(
            "HUBSPOT_APP_RATE_PER_10S",
            std::env::var("HUBSPOT_APP_RATE_PER_10S").ok(),
            u64::from(DEFAULT_PER_10S),
            1,
            190,
        ) as u32;
        let search_per_second = env_u64(
            "HUBSPOT_APP_SEARCH_PER_SEC",
            std::env::var("HUBSPOT_APP_SEARCH_PER_SEC").ok(),
            u64::from(DEFAULT_SEARCH_PER_SECOND),
            1,
            5,
        );
        let interactive = env_u64(
            "HUBSPOT_INTERACTIVE_MAX_WAIT_MS",
            std::env::var("HUBSPOT_INTERACTIVE_MAX_WAIT_MS").ok(),
            DEFAULT_INTERACTIVE_MAX_WAIT_MS,
            100,
            60_000,
        );
        let background = env_u64(
            "HUBSPOT_BACKGROUND_MAX_WAIT_MS",
            std::env::var("HUBSPOT_BACKGROUND_MAX_WAIT_MS").ok(),
            DEFAULT_BACKGROUND_MAX_WAIT_MS,
            100,
            600_000,
        );
        Self {
            per_second,
            per_10s,
            search_interval: Duration::from_millis(1000 / search_per_second),
            interactive_max_wait: Duration::from_millis(interactive),
            background_max_wait: Duration::from_millis(background),
            ..Self::default()
        }
    }

    /// 流量の制限なし (テストの既定。Search の間隔と 429 の最短停止だけ指定する)
    pub fn unlimited(search_interval: Duration, rate_limited_min_pause: Duration) -> Self {
        Self {
            per_second: 0,
            per_10s: 0,
            search_interval,
            // 待ちの上限は実質なし (テストの偽 HubSpot に合わせて呼び出し側の timeout で切る)
            interactive_max_wait: Duration::from_secs(24 * 60 * 60),
            background_max_wait: Duration::from_secs(24 * 60 * 60),
            rate_limited_min_pause,
            max_pause: MAX_PAUSE,
        }
    }

    pub fn max_wait(&self, p: Priority) -> Duration {
        match p {
            Priority::Interactive => self.interactive_max_wait,
            Priority::Background => self.background_max_wait,
        }
    }

    fn windows(&self, lane: Lane) -> Vec<(Duration, u32)> {
        match lane {
            Lane::General => {
                let mut w = Vec::new();
                if self.per_second > 0 {
                    w.push((Duration::from_secs(1), self.per_second));
                }
                if self.per_10s > 0 {
                    w.push((Duration::from_secs(10), self.per_10s));
                }
                w
            }
            Lane::Search if self.search_interval.is_zero() => Vec::new(),
            Lane::Search => vec![(self.search_interval, 1)],
        }
    }
}

// ---------------------------------------------------------------------------
// 状態
// ---------------------------------------------------------------------------

struct LaneState {
    /// (窓の長さ, 窓の中の上限)
    windows: Vec<(Duration, u32)>,
    /// 直近の許可時刻 (古い順。最長の窓より古いものは捨てる)
    grants: VecDeque<Instant>,
    horizon: Duration,
    /// 1 回あたりの最短間隔の見積もり (並んだ時点の待ちの見積もりに使う)
    spacing: Duration,
    interactive: VecDeque<u64>,
    background: VecDeque<u64>,
}

impl LaneState {
    fn new(windows: Vec<(Duration, u32)>) -> Self {
        let horizon = windows.iter().map(|(w, _)| *w).max().unwrap_or_default();
        let spacing = windows
            .iter()
            .map(|(w, m)| *w / (*m).max(1))
            .max()
            .unwrap_or_default();
        Self {
            windows,
            grants: VecDeque::new(),
            horizon,
            spacing,
            interactive: VecDeque::new(),
            background: VecDeque::new(),
        }
    }

    fn queue(&mut self, p: Priority) -> &mut VecDeque<u64> {
        match p {
            Priority::Interactive => &mut self.interactive,
            Priority::Background => &mut self.background,
        }
    }

    fn prune(&mut self, now: Instant) {
        if let Some(cut) = now.checked_sub(self.horizon) {
            while self.grants.front().is_some_and(|g| *g <= cut) {
                self.grants.pop_front();
            }
        }
    }

    /// 次に許可できる最も早い時刻 (窓だけを見る。停止は呼び出し側が足す)。
    /// 窓 (w, m) では、直近 m 件の最古の許可から w 経てば 1 件空く (区間 (t-w, t] で数える)
    fn window_ready_at(&self, now: Instant) -> Instant {
        let mut t = now;
        let n = self.grants.len();
        for (w, m) in &self.windows {
            let m = *m as usize;
            if m > 0 && n >= m {
                t = t.max(self.grants[n - m] + *w);
            }
        }
        t
    }

    /// この券の前に並んでいる数 (優先度順)。列に無ければ None
    fn ahead_of(&self, p: Priority, ticket: u64) -> Option<usize> {
        match p {
            Priority::Interactive => self.interactive.iter().position(|t| *t == ticket),
            Priority::Background => self
                .background
                .iter()
                .position(|t| *t == ticket)
                .map(|i| i + self.interactive.len()),
        }
    }

    fn depth(&self) -> usize {
        self.interactive.len() + self.background.len()
    }
}

struct State {
    lanes: [LaneState; 2],
    paused_until: Option<Instant>,
    next_ticket: u64,
}

/// 数えるもの (起動してからの合計)
#[derive(Default)]
struct Counters {
    calls: Mutex<BTreeMap<&'static str, u64>>,
    search_calls: AtomicU64,
    rate_limited: AtomicU64,
    coalesced: AtomicU64,
    busy: AtomicU64,
    granted: AtomicU64,
}

/// 最後に見た `X-HubSpot-RateLimit-*` (HubSpot の応答ヘッダの値そのまま)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimitHeaders {
    /// 観測した時刻 (RFC 3339)
    pub observed_at: Option<String>,
    /// `x-hubspot-ratelimit-max` (10 秒あたりの上限)
    pub max: Option<u64>,
    /// `x-hubspot-ratelimit-remaining` (今の 10 秒の残り)
    pub remaining: Option<u64>,
    /// `x-hubspot-ratelimit-interval-milliseconds`
    pub interval_ms: Option<u64>,
    /// `x-hubspot-ratelimit-secondly`
    pub secondly: Option<u64>,
    /// `x-hubspot-ratelimit-secondly-remaining`
    pub secondly_remaining: Option<u64>,
    /// `x-hubspot-ratelimit-daily`
    pub daily: Option<u64>,
    /// `x-hubspot-ratelimit-daily-remaining`
    pub daily_remaining: Option<u64>,
}

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// ある時点の数字 (`/api/admin/hubspot-usage` が JSON にする)
#[derive(Debug, Clone)]
pub struct GatewaySnapshot {
    pub config: GatewayConfig,
    pub calls_by_group: BTreeMap<&'static str, u64>,
    pub total_calls: u64,
    pub search_calls: u64,
    pub rate_limited: u64,
    pub coalesced: u64,
    pub busy: u64,
    pub granted: u64,
    pub queue_general: usize,
    pub queue_search: usize,
    /// 429 で止めている残り時間 (止めていなければ None)
    pub paused_for: Option<Duration>,
    pub wait_p50: Option<Duration>,
    pub wait_p95: Option<Duration>,
    pub wait_samples: usize,
    pub rate_limit: RateLimitHeaders,
}

pub struct Gateway {
    cfg: GatewayConfig,
    state: Mutex<State>,
    notify: Notify,
    counters: Counters,
    waits: Mutex<VecDeque<(Instant, Duration)>>,
    rate_limit: Mutex<RateLimitHeaders>,
}

impl std::fmt::Debug for Gateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gateway")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

static SHARED: OnceLock<Arc<Gateway>> = OnceLock::new();

/// 列から抜ける (許可・断り・取り消し) ときに必ず券を消す
struct TicketGuard<'a> {
    gw: &'a Gateway,
    lane: Lane,
    priority: Priority,
    ticket: u64,
    active: bool,
}

impl Drop for TicketGuard<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut st) = self.gw.state.lock() {
            let q = st.lanes[self.lane.idx()].queue(self.priority);
            q.retain(|t| *t != self.ticket);
        }
        // 先頭が変わったかもしれないので待っている人に知らせる
        self.gw.notify.notify_waiters();
    }
}

impl Gateway {
    pub fn new(cfg: GatewayConfig) -> Self {
        let lanes = [
            LaneState::new(cfg.windows(Lane::General)),
            LaneState::new(cfg.windows(Lane::Search)),
        ];
        Self {
            cfg,
            state: Mutex::new(State {
                lanes,
                paused_until: None,
                next_ticket: 0,
            }),
            notify: Notify::new(),
            counters: Counters::default(),
            waits: Mutex::new(VecDeque::new()),
            rate_limit: Mutex::new(RateLimitHeaders::default()),
        }
    }

    /// プロセスで 1 つの関所 (環境変数の設定)。本番の HubSpot クライアントは全部これを使う
    pub fn shared() -> Arc<Gateway> {
        SHARED
            .get_or_init(|| {
                let cfg = GatewayConfig::from_env();
                tracing::info!(
                    per_second = cfg.per_second,
                    per_10s = cfg.per_10s,
                    search_interval_ms = cfg.search_interval.as_millis() as u64,
                    interactive_max_wait_ms = cfg.interactive_max_wait.as_millis() as u64,
                    background_max_wait_ms = cfg.background_max_wait.as_millis() as u64,
                    "HubSpot 呼び出しの関所を初期化しました"
                );
                Arc::new(Gateway::new(cfg))
            })
            .clone()
    }

    pub fn config(&self) -> &GatewayConfig {
        &self.cfg
    }

    /// 1 回の HubSpot 呼び出しの許可を待つ。待てた時間を返す。
    /// 待ちの上限 (優先度ごと) を超える・超えると見込まれるなら `Err(Busy)` (呼び出しはしない)。
    pub async fn acquire(&self, lane: Lane, priority: Priority) -> Result<Duration, Busy> {
        let max_wait = self.cfg.max_wait(priority);
        let started = Instant::now();
        let deadline = started + max_wait;
        let mut guard = {
            let Ok(mut st) = self.state.lock() else {
                return Err(Busy);
            };
            let ticket = st.next_ticket;
            st.next_ticket += 1;
            let paused = st.paused_until;
            let ls = &mut st.lanes[lane.idx()];
            ls.prune(started);
            ls.queue(priority).push_back(ticket);
            // 並んだ時点の見積もり: 窓と停止が空くまで + 前に並んでいる数 × 最短間隔
            let ahead = ls.ahead_of(priority, ticket).unwrap_or(0) as u32;
            let mut ready = ls.window_ready_at(started);
            if let Some(p) = paused {
                ready = ready.max(p);
            }
            let estimate = (ready - started) + ls.spacing.saturating_mul(ahead);
            if estimate > max_wait {
                ls.queue(priority).retain(|t| *t != ticket);
                drop(st);
                self.counters.busy.fetch_add(1, Ordering::Relaxed);
                return Err(Busy);
            }
            TicketGuard {
                gw: self,
                lane,
                priority,
                ticket,
                active: true,
            }
        };
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            // ロックを離す前に登録する (その間の知らせを取りこぼさない)
            notified.as_mut().enable();
            let wake = {
                let Ok(mut st) = self.state.lock() else {
                    return Err(Busy);
                };
                let now = Instant::now();
                let paused = st.paused_until;
                let ls = &mut st.lanes[lane.idx()];
                ls.prune(now);
                let head = ls.ahead_of(priority, guard.ticket) == Some(0);
                if head {
                    let mut ready = ls.window_ready_at(now);
                    if let Some(p) = paused {
                        ready = ready.max(p);
                    }
                    if ready <= now {
                        ls.grants.push_back(now);
                        ls.queue(priority).retain(|t| *t != guard.ticket);
                        guard.active = false;
                        drop(st);
                        self.notify.notify_waiters();
                        let waited = now - started;
                        self.record_wait(now, waited);
                        self.counters.granted.fetch_add(1, Ordering::Relaxed);
                        return Ok(waited);
                    }
                    ready
                } else {
                    deadline
                }
            };
            if Instant::now() >= deadline {
                // guard の Drop が列から外して知らせる
                drop(guard);
                self.counters.busy.fetch_add(1, Ordering::Relaxed);
                return Err(Busy);
            }
            tokio::select! {
                _ = tokio::time::sleep_until(wake.min(deadline)) => {}
                _ = &mut notified => {}
            }
        }
    }

    fn record_wait(&self, now: Instant, waited: Duration) {
        if let Ok(mut w) = self.waits.lock() {
            w.push_back((now, waited));
            while w.len() > WAIT_SAMPLES_MAX {
                w.pop_front();
            }
        }
    }

    /// 429 を受けた。`Retry-After` (無ければ最低待ち時間) の間、全員を止める (上限 `max_pause`)
    pub fn on_rate_limited(&self, retry_after: Option<Duration>) {
        self.counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        let pause = retry_after
            .unwrap_or_default()
            .max(self.cfg.rate_limited_min_pause)
            .min(self.cfg.max_pause);
        let until = Instant::now() + pause;
        if let Ok(mut st) = self.state.lock() {
            st.paused_until = Some(st.paused_until.map_or(until, |p| p.max(until)));
        }
        tracing::warn!(
            pause_ms = pause.as_millis() as u64,
            "HubSpot が 429 を返したため、全ての呼び出しを止めます"
        );
    }

    /// 実際に HubSpot へ送った 1 回 (retry も 1 回と数える)
    pub fn record_call(&self, group: &'static str, lane: Lane) {
        if lane == Lane::Search {
            self.counters.search_calls.fetch_add(1, Ordering::Relaxed);
        }
        if let Ok(mut m) = self.counters.calls.lock() {
            *m.entry(group).or_default() += 1;
        }
    }

    /// 同じ読み取りに相乗りした (HubSpot を呼ばずに済んだ) 1 回
    pub fn record_coalesced(&self) {
        self.counters.coalesced.fetch_add(1, Ordering::Relaxed);
    }

    /// 応答ヘッダの `X-HubSpot-RateLimit-*` を記録する (1 つも無ければ前回値を残す)
    pub fn record_headers(&self, headers: &HeaderMap) {
        let snap = RateLimitHeaders {
            observed_at: None,
            max: header_u64(headers, "x-hubspot-ratelimit-max"),
            remaining: header_u64(headers, "x-hubspot-ratelimit-remaining"),
            interval_ms: header_u64(headers, "x-hubspot-ratelimit-interval-milliseconds"),
            secondly: header_u64(headers, "x-hubspot-ratelimit-secondly"),
            secondly_remaining: header_u64(headers, "x-hubspot-ratelimit-secondly-remaining"),
            daily: header_u64(headers, "x-hubspot-ratelimit-daily"),
            daily_remaining: header_u64(headers, "x-hubspot-ratelimit-daily-remaining"),
        };
        if snap == RateLimitHeaders::default() {
            return;
        }
        if let Ok(mut g) = self.rate_limit.lock() {
            // 片方だけのヘッダ (Search の応答など) で、もう片方の前回値を消さない
            let keep = |new: Option<u64>, old: Option<u64>| new.or(old);
            *g = RateLimitHeaders {
                observed_at: Some(
                    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                ),
                max: keep(snap.max, g.max),
                remaining: keep(snap.remaining, g.remaining),
                interval_ms: keep(snap.interval_ms, g.interval_ms),
                secondly: keep(snap.secondly, g.secondly),
                secondly_remaining: keep(snap.secondly_remaining, g.secondly_remaining),
                daily: keep(snap.daily, g.daily),
                daily_remaining: keep(snap.daily_remaining, g.daily_remaining),
            };
        }
    }

    pub fn snapshot(&self) -> GatewaySnapshot {
        let now = Instant::now();
        let (queue_general, queue_search, paused_for) = match self.state.lock() {
            Ok(st) => (
                st.lanes[0].depth(),
                st.lanes[1].depth(),
                st.paused_until.filter(|p| *p > now).map(|p| p - now),
            ),
            Err(_) => (0, 0, None),
        };
        let mut recent: Vec<Duration> = match self.waits.lock() {
            Ok(mut w) => {
                if let Some(cut) = now.checked_sub(WAIT_WINDOW) {
                    while w.front().is_some_and(|(at, _)| *at < cut) {
                        w.pop_front();
                    }
                }
                w.iter().map(|(_, d)| *d).collect()
            }
            Err(_) => Vec::new(),
        };
        recent.sort();
        let pct = |p: f64| -> Option<Duration> {
            if recent.is_empty() {
                return None;
            }
            // 最近順位法 (nearest-rank)
            let rank = ((p * recent.len() as f64).ceil() as usize).clamp(1, recent.len());
            Some(recent[rank - 1])
        };
        let calls_by_group = self
            .counters
            .calls
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default();
        GatewaySnapshot {
            config: self.cfg.clone(),
            total_calls: calls_by_group.values().sum(),
            calls_by_group,
            search_calls: self.counters.search_calls.load(Ordering::Relaxed),
            rate_limited: self.counters.rate_limited.load(Ordering::Relaxed),
            coalesced: self.counters.coalesced.load(Ordering::Relaxed),
            busy: self.counters.busy.load(Ordering::Relaxed),
            granted: self.counters.granted.load(Ordering::Relaxed),
            queue_general,
            queue_search,
            paused_for,
            wait_p50: pct(0.50),
            wait_p95: pct(0.95),
            wait_samples: recent.len(),
            rate_limit: self
                .rate_limit
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default(),
        }
    }
}

/// パスから呼び出しの種類 (数える単位) を決める
pub fn endpoint_group(path: &str) -> &'static str {
    let p = path.split('?').next().unwrap_or(path);
    if p.starts_with("/oauth/") {
        "token_info"
    } else if p.ends_with("/search") {
        "search"
    } else if p.starts_with("/crm/v4/associations/") && p.ends_with("/batch/read") {
        "associations_batch"
    } else if p.starts_with("/crm/v4/") {
        "associations"
    } else if p.ends_with("/batch/read") {
        "batch_read"
    } else if p.starts_with("/crm/v3/objects/") {
        "object_read"
    } else if p.starts_with("/crm/v3/owners") {
        "owners"
    } else if p.starts_with("/crm/v3/properties/") {
        "properties"
    } else if p.starts_with("/crm/v3/pipelines/") {
        "pipelines"
    } else {
        "other"
    }
}

// ---------------------------------------------------------------------------
// キャッシュの当たり外れ (プロセス全体。`/api/admin/hubspot-usage` が読む)
// ---------------------------------------------------------------------------

static CACHE_STATS: OnceLock<Mutex<BTreeMap<&'static str, (u64, u64)>>> = OnceLock::new();

fn cache_stats() -> &'static Mutex<BTreeMap<&'static str, (u64, u64)>> {
    CACHE_STATS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// キャッシュから返せた (HubSpot を呼ばずに済んだ)
pub fn cache_hit(name: &'static str) {
    if let Ok(mut m) = cache_stats().lock() {
        m.entry(name).or_default().0 += 1;
    }
}

/// キャッシュに無かった・古かった (HubSpot を呼んだ)
pub fn cache_miss(name: &'static str) {
    if let Ok(mut m) = cache_stats().lock() {
        m.entry(name).or_default().1 += 1;
    }
}

/// キャッシュ名 → (当たり, 外れ)
pub fn cache_stats_snapshot() -> BTreeMap<&'static str, (u64, u64)> {
    cache_stats().lock().map(|m| m.clone()).unwrap_or_default()
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod gateway_tests;
