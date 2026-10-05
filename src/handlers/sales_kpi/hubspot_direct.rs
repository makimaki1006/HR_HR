//! 営業KPI: HubSpot 直読みの常駐キャッシュと、`Sheets` への組み立て (2026-10-05)
//!
//! 有効になるのは環境変数 `SALES_KPI_HUBSPOT_DIRECT=1` のときだけ。**既定は従来どおりシート**で、
//! 本番の挙動は変わらない。有効にしても、直読みするのは `hubspot_source` の 6 ブロックだけで、
//! 残り(担当別・リスト在庫・架電日次・週次・決定者の過去日・取得条件の Zoom 項目)は
//! 引き続きシートから読む(混在)。
//!
//! ## リクエストの中で HubSpot を叩かない
//! 背景タスクが `REFRESH_INTERVAL`(5 分)ごとに更新する。リクエストは常駐の最新値を読むだけ。
//! - 同時に 2 本走らない(`flight` ロックの `try_lock`。走っている間は新しい更新を始めない)
//! - デプロイ直後は値が無い: `HubSpot取得状態=loading` を `meta` に入れて返す
//!   (HubSpot 由来の 6 ブロックは空。画面は 0 件ではなく「取得中」と読む)
//! - 失敗したら直前の値を出し続け、`HubSpot取得状態=stale` と失敗の種別・時刻・
//!   「何時何分の値か」を `meta` に入れる。**黙ってシートに戻さない**
//! - 値が 1 度も取れないまま失敗したら 503(理由つき JSON)
//! - 画面を開く人が 30 分いなければ背景更新を止める(次のリクエストで再開)
//!
//! `meta` に足すキー(いずれも文字列。型は変わらない):
//! `データ元` / `HubSpot取得状態`(ok|stale|loading) / `HubSpot取得時刻` / `HubSpot最終失敗種別` /
//! `HubSpot最終失敗時刻` / `HubSpot打ち切り` / `HubSpot更新のリクエスト数` / `HubSpot更新の所要秒`。
//! 既存キーのうち `商談の範囲` / `当月` / `今週のはじまり` / `取得時刻` / `取得元` は HubSpot の値で上書きする。

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{FixedOffset, Utc};

use super::hubspot_source::{
    exclusions_from_sheet, roster_from_member_sheet, DirectBlocks, FetchError, KETTEI_HEADER,
};
use super::{
    empty_sheet, Sheets, SHEET_KADEN, SHEET_KADEN_BY_OWNER, SHEET_KETTEI, SHEET_LIST_STOCK,
    SHEET_MEMBER, SHEET_META, SHEET_WEEKLY,
};
use crate::config::HubSpotApiConfig;
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::routes::{cq_state, CqError};
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};
use crate::hubspot::{ClientOptions, HubSpotClient, DEFAULT_BASE_URL};

/// 背景更新の間隔。
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(300);
/// これだけ誰も画面を開いていなければ背景更新を止める。
pub const IDLE_STOP: Duration = Duration::from_secs(30 * 60);
/// `?refresh=1` が強制更新を起こす最短の間隔(同じ鍵への連打を防ぐ)。
pub const FORCED_REFRESH_MIN_GAP: Duration = Duration::from_secs(60);
/// 1 回の更新の上限(これを超えたら失敗として次の周期に回す)。
const UPDATE_TIMEOUT: Duration = Duration::from_secs(600);

/// 手入力の運用シート(名簿ではない)。HubSpot チーム名または ownerId で商談の集計から外す。
pub const SHEET_EXCLUDE: &str = "KPI営業_集計除外";

pub type FetchFuture = Pin<Box<dyn Future<Output = Result<DirectBlocks, FetchError>> + Send>>;
/// 1 回ぶんの更新(HubSpot へ取りに行く処理)。テストでは偽物を渡す。
pub type FetchFn = Arc<dyn Fn() -> FetchFuture + Send + Sync>;

/// `SALES_KPI_HUBSPOT_DIRECT` が `1` / `true` か。
pub fn enabled() -> bool {
    // 🔴 tests/env_example_matches_code.rs が `env::var("NAME")` の文字列リテラルを拾うので直書き
    matches!(
        std::env::var("SALES_KPI_HUBSPOT_DIRECT")
            .as_deref()
            .map(str::trim),
        Ok("1") | Ok("true")
    )
}

/// 直近の失敗。HubSpot の応答本文・トークンは持たない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrInfo {
    pub kind: &'static str,
    /// 失敗した時刻 `yyyy-MM-dd HH:mm`(JST)
    pub at: String,
}

/// 画面に出す状態。
#[derive(Debug, Clone)]
pub enum Snapshot {
    /// まだ 1 度も取れていない(取得中)
    Loading,
    /// 1 度も取れないまま失敗した
    NoValue(ErrInfo),
    /// 値がある。`error` があれば直近の更新は失敗(= 古い値)
    Ready {
        blocks: Arc<DirectBlocks>,
        error: Option<ErrInfo>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Updated,
    Failed(&'static str),
    /// 別の更新が走っていたので始めなかった
    AlreadyRunning,
}

#[derive(Default)]
struct Inner {
    blocks: Option<Arc<DirectBlocks>>,
    last_error: Option<ErrInfo>,
    last_started: Option<Instant>,
}

pub struct DirectState {
    inner: Mutex<Inner>,
    /// 更新を同時に 2 本走らせないための鍵。更新の間ずっと持つ
    flight: tokio::sync::Mutex<()>,
    /// 最後にリクエストが来た時刻(UNIX 秒)
    last_request: AtomicU64,
    loop_active: AtomicBool,
    fetch: FetchFn,
    interval: Duration,
    idle_stop: Duration,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_text() -> String {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(9 * 3600).expect("JST"))
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

impl DirectState {
    pub fn new(fetch: FetchFn) -> Arc<Self> {
        Self::with_timing(fetch, REFRESH_INTERVAL, IDLE_STOP)
    }

    pub fn with_timing(fetch: FetchFn, interval: Duration, idle_stop: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            flight: tokio::sync::Mutex::new(()),
            last_request: AtomicU64::new(0),
            loop_active: AtomicBool::new(false),
            fetch,
            interval,
            idle_stop,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        let g = self.inner.lock().expect("lock");
        match (&g.blocks, &g.last_error) {
            (Some(b), e) => Snapshot::Ready {
                blocks: Arc::clone(b),
                error: e.clone(),
            },
            (None, Some(e)) => Snapshot::NoValue(e.clone()),
            (None, None) => Snapshot::Loading,
        }
    }

    /// 更新を 1 回走らせる。**すでに走っていれば何もしない**(同時に 2 本走らせない)。
    /// 失敗しても直前の値は消さない。
    pub async fn run_once(&self) -> RunOutcome {
        let Ok(_flight) = self.flight.try_lock() else {
            return RunOutcome::AlreadyRunning;
        };
        self.inner.lock().expect("lock").last_started = Some(Instant::now());
        let res = match tokio::time::timeout(UPDATE_TIMEOUT, (self.fetch)()).await {
            Ok(r) => r,
            Err(_) => Err(FetchError {
                kind: "update_timeout",
                detail: "更新が時間内に終わりませんでした".to_string(),
            }),
        };
        let mut g = self.inner.lock().expect("lock");
        match res {
            Ok(blocks) => {
                tracing::info!(
                    requests = blocks.requests,
                    took_secs = blocks.took_secs,
                    "営業KPI: HubSpot 直読みを更新しました"
                );
                g.blocks = Some(Arc::new(blocks));
                g.last_error = None;
                RunOutcome::Updated
            }
            Err(e) => {
                tracing::warn!(
                    error_kind = e.kind,
                    "営業KPI: HubSpot 直読みの更新に失敗しました(直前の値を出し続けます): {}",
                    e.detail
                );
                g.last_error = Some(ErrInfo {
                    kind: e.kind,
                    at: now_text(),
                });
                RunOutcome::Failed(e.kind)
            }
        }
    }

    /// リクエストが来たことを記録し、背景更新が止まっていれば起こす。
    pub fn touch(self: &Arc<Self>) {
        self.last_request.store(now_secs(), Ordering::Relaxed);
        self.start_loop_if_stopped();
    }

    fn start_loop_if_stopped(self: &Arc<Self>) {
        if self
            .loop_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let me = Arc::clone(self);
            tokio::spawn(async move { me.run_loop().await });
        }
    }

    fn idle(&self) -> bool {
        now_secs().saturating_sub(self.last_request.load(Ordering::Relaxed))
            > self.idle_stop.as_secs()
    }

    async fn run_loop(self: Arc<Self>) {
        loop {
            let _ = self.run_once().await;
            tokio::time::sleep(self.interval).await;
            if self.idle() {
                self.loop_active.store(false, Ordering::Release);
                // 止める直前に来たリクエストを取りこぼさない
                if !self.idle()
                    && self
                        .loop_active
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    continue;
                }
                return;
            }
        }
    }

    /// `?refresh=1`: 直前の更新開始から `FORCED_REFRESH_MIN_GAP` 以上たっていれば、
    /// 背景で更新を 1 回起こす(リクエストは待たない)。戻り値は起こしたか。
    pub fn request_refresh(self: &Arc<Self>) -> bool {
        let recent = self
            .inner
            .lock()
            .expect("lock")
            .last_started
            .is_some_and(|t| t.elapsed() < FORCED_REFRESH_MIN_GAP);
        if recent {
            return false;
        }
        let me = Arc::clone(self);
        tokio::spawn(async move {
            let _ = me.run_once().await;
        });
        true
    }
}

// ---------------------------------------------------------------- 本番の配線

static DIRECT: OnceLock<Option<Arc<DirectState>>> = OnceLock::new();

/// 営業KPI 用の HubSpot クライアント(`AppState.hubspot` の CRM 用とは別インスタンス)。
///
/// Search の間隔は 400ms(2.5 回/秒)。CRM(1 回/秒)と合わせても 5 回/秒の上限に収まる。
/// ただし同じトークンを共有する他の常駐処理・Python の同期とは別のゲートなので、
/// 全体では上限を超えうる(429 は `Retry-After` に従って待つ)。
fn build_client() -> Option<HubSpotClient> {
    let cfg = HubSpotApiConfig::from_env()?;
    HubSpotClient::new(
        cfg.access_token,
        DEFAULT_BASE_URL,
        ClientOptions {
            timeout: Duration::from_secs(30),
            max_retries: 6,
            search_min_interval: Duration::from_millis(400),
            ..ClientOptions::default()
        },
    )
    .ok()
}

fn global() -> Option<&'static Arc<DirectState>> {
    DIRECT
        .get_or_init(|| {
            let client = Arc::new(build_client()?);
            let fetch: FetchFn = Arc::new(move || {
                let client = Arc::clone(&client);
                Box::pin(async move {
                    let st = cq_state().map_err(|_| FetchError {
                        kind: "sheets_unavailable",
                        detail: "Sheets が使えません".to_string(),
                    })?;
                    let (roster, excl) = read_manual_inputs(&st.client, &st.store).await?;
                    let now =
                        Utc::now().with_timezone(&FixedOffset::east_opt(9 * 3600).expect("JST"));
                    super::hubspot_source::fetch_blocks(&client, now, &roster, &excl).await
                })
            });
            Some(DirectState::new(fetch))
        })
        .as_ref()
}

/// 名簿(前回のメンバーシートの「出どころ=名簿」)と、集計除外シート。読めなければ更新を失敗にする
/// (名簿や除外が欠けた集計を、気づかれないまま出さないため)。
async fn read_manual_inputs(
    client: &SheetsClient,
    store: &SheetStore,
) -> Result<
    (
        std::collections::BTreeMap<String, super::hubspot_source::RosterEntry>,
        super::hubspot_source::Exclusions,
    ),
    FetchError,
> {
    let member = store
        .get(client, SHEET_MEMBER)
        .await
        .map_err(|e| FetchError {
            kind: "roster_unavailable",
            detail: format!("メンバーシートが読めません: {e:#}"),
        })?;
    let excl = store
        .get(client, SHEET_EXCLUDE)
        .await
        .map_err(|e| FetchError {
            kind: "exclusions_unavailable",
            detail: format!("集計除外シートが読めません: {e:#}"),
        })?;
    Ok((
        roster_from_member_sheet(&member.0),
        exclusions_from_sheet(&excl.0),
    ))
}

// ---------------------------------------------------------------- Sheets の組み立て

fn rows_of(pairs: &[(String, String)]) -> Vec<Vec<Arc<str>>> {
    pairs
        .iter()
        .map(|(k, v)| vec![Arc::from(k.as_str()), Arc::from(v.as_str())])
        .collect()
}

/// シートの `KPI営業_取得条件`(Zoom の項目など)に、HubSpot 直読みの項目を重ねる。
/// 既存の項目は値だけ差し替え、無ければ末尾に足す。
pub fn overlay_meta(base: &SheetData, snap: &Snapshot) -> SheetData {
    let mut pairs: Vec<(String, String)> = base
        .rows
        .iter()
        .map(|r| {
            (
                base.get(r, "項目").to_string(),
                base.get(r, "値").to_string(),
            )
        })
        .collect();
    let mut set = |k: &str, v: String| {
        if let Some(p) = pairs.iter_mut().find(|p| p.0 == k) {
            p.1 = v;
        } else {
            pairs.push((k.to_string(), v));
        }
    };
    set(
        "データ元",
        "HubSpot直読み(商談・アポ・Cヨミ・決定者の当日分・メンバー・架電リスト)。\
         担当別・リスト在庫・架電日次・週次・決定者の過去日はシート"
            .to_string(),
    );
    match snap {
        Snapshot::Loading | Snapshot::NoValue(_) => {
            set("HubSpot取得状態", "loading".to_string());
            set("HubSpot取得時刻", String::new());
            set("取得時刻", String::new());
            set("取得元", "HubSpot直読み(取得中)".to_string());
        }
        Snapshot::Ready { blocks, error } => {
            let w = &blocks.windows;
            set("商談の範囲", format!("{} 〜 {}", w.lo, w.hi));
            set("当月", w.today.format("%Y-%m").to_string());
            set("今週のはじまり", w.week_start.to_string());
            set("取得時刻", blocks.fetched_at.clone());
            set("取得元", "HubSpot直読み(hubspot_source.rs)".to_string());
            set("HubSpot取得時刻", blocks.fetched_at.clone());
            set("HubSpot更新のリクエスト数", blocks.requests.to_string());
            set("HubSpot更新の所要秒", blocks.took_secs.to_string());
            match error {
                None => set("HubSpot取得状態", "ok".to_string()),
                Some(e) => {
                    set("HubSpot取得状態", "stale".to_string());
                    set("HubSpot最終失敗種別", e.kind.to_string());
                    set("HubSpot最終失敗時刻", e.at.clone());
                }
            }
            if !blocks.truncated.is_empty() {
                set("HubSpot打ち切り", blocks.truncated.join(","));
            }
        }
    }
    SheetData {
        header: vec!["項目".to_string(), "値".to_string()],
        rows: rows_of(&pairs),
        fetched_at: base.fetched_at,
    }
}

/// シートの決定者(全日)に、HubSpot の当日分を重ねる。当日の行はシート側を捨てて入れ替える
/// (Python の `upsert_sheet` が `日付 + ownerId` をキーに同じ日を上書きするのと同じ)。
pub fn merge_kettei(sheet: &SheetData, day: &str, live: &[Vec<String>]) -> SheetData {
    let header: Vec<String> = if sheet.header.is_empty() {
        KETTEI_HEADER.iter().map(|s| s.to_string()).collect()
    } else {
        sheet.header.clone()
    };
    let live_header: Vec<&str> = KETTEI_HEADER.to_vec();
    let mut rows: Vec<Vec<Arc<str>>> = sheet
        .rows
        .iter()
        .filter(|r| sheet.get(r, "日付").trim() != day)
        .cloned()
        .collect();
    for l in live {
        rows.push(
            header
                .iter()
                .map(|h| {
                    let h = h.trim_start_matches('\u{feff}').trim();
                    let v = live_header
                        .iter()
                        .position(|x| *x == h)
                        .and_then(|i| l.get(i))
                        .map(String::as_str)
                        .unwrap_or("");
                    Arc::<str>::from(v)
                })
                .collect(),
        );
    }
    // シート上の並びは Python の upsert が (日付, ownerId) 順に直す
    let di = header.iter().position(|h| h.trim() == "日付");
    let oi = header.iter().position(|h| h.trim() == "ownerId");
    if let (Some(di), Some(oi)) = (di, oi) {
        rows.sort_by(|a, b| (&a[di], &a[oi]).cmp(&(&b[di], &b[oi])));
    }
    SheetData {
        header,
        rows,
        fetched_at: sheet.fetched_at,
    }
}

/// 常駐の最新値とシートから `Sheets` を組む。HubSpot 由来の 6 ブロックだけ差し替える。
pub async fn assemble(
    client: &SheetsClient,
    store: &SheetStore,
    snap: &Snapshot,
) -> Result<Sheets> {
    let mut cached = true;
    macro_rules! fetch {
        ($name:expr) => {{
            let (data, hit) = store
                .get(client, $name)
                .await
                .with_context(|| format!("シート「{}」が読めません", $name))?;
            cached &= hit;
            data
        }};
    }
    macro_rules! optional {
        ($name:expr, $what:expr) => {
            match store.get(client, $name).await {
                Ok((data, hit)) => {
                    cached &= hit;
                    data
                }
                Err(e) => {
                    tracing::warn!(
                        "シート「{}」が読めないので{}は空で出します: {e:#}",
                        $name,
                        $what
                    );
                    empty_sheet()
                }
            }
        };
    }
    let kaden = fetch!(SHEET_KADEN);
    let kaden_by_owner = optional!(SHEET_KADEN_BY_OWNER, "架電リストの担当者別");
    let weekly = optional!(SHEET_WEEKLY, "週次");
    let kettei_sheet = optional!(SHEET_KETTEI, "決定者・決裁者の過去日");
    let list_stock = optional!(SHEET_LIST_STOCK, "リストの在庫");
    let meta_sheet = optional!(SHEET_META, "架電(Zoom)の取得条件");
    Ok(build_sheets(
        snap,
        kaden,
        kaden_by_owner,
        weekly,
        &kettei_sheet,
        list_stock,
        &meta_sheet,
        cached,
    ))
}

/// 読み終えたシートと状態から `Sheets` を作る(I/O なし。テストから直接呼ぶ)。
#[allow(clippy::too_many_arguments)]
pub fn build_sheets(
    snap: &Snapshot,
    kaden: Arc<SheetData>,
    kaden_by_owner: Arc<SheetData>,
    weekly: Arc<SheetData>,
    kettei_sheet: &SheetData,
    list_stock: Arc<SheetData>,
    meta_sheet: &SheetData,
    all_cached: bool,
) -> Sheets {
    let meta = Arc::new(overlay_meta(meta_sheet, snap));
    match snap {
        Snapshot::Ready { blocks, .. } => Sheets {
            shodan: Arc::clone(&blocks.shodan),
            apo: Arc::clone(&blocks.apo),
            cyomi: Arc::clone(&blocks.cyomi),
            kaden,
            kaden_list: Arc::clone(&blocks.kaden_list),
            kaden_by_owner,
            member: Arc::clone(&blocks.member),
            meta,
            weekly,
            kettei: Arc::new(merge_kettei(
                kettei_sheet,
                &blocks.kettei_day,
                &blocks.kettei_rows,
            )),
            list_stock,
            all_cached,
        },
        // 取得中: HubSpot 由来の 6 ブロックは空。メンバーが空でも payload は組める。
        // 決定者の過去日はシートのまま出す(当日分だけ無い)
        Snapshot::Loading | Snapshot::NoValue(_) => Sheets {
            shodan: empty_sheet(),
            apo: empty_sheet(),
            cyomi: empty_sheet(),
            kaden,
            kaden_list: empty_sheet(),
            kaden_by_owner,
            member: empty_sheet(),
            meta,
            weekly,
            kettei: Arc::new(SheetData {
                header: kettei_sheet.header.clone(),
                rows: kettei_sheet.rows.clone(),
                fetched_at: kettei_sheet.fetched_at,
            }),
            list_stock,
            all_cached,
        },
    }
}

/// `data()` から呼ぶ。直読みが有効なときの応答。
pub async fn respond(refresh: bool) -> Result<Response, CqError> {
    let Some(direct) = global() else {
        return Err(CqError::hubspot_direct(
            "hubspot_not_configured",
            "SALES_KPI_HUBSPOT_DIRECT=1 ですが HUBSPOT_ACCESS_TOKEN が未設定(または不正)です。\
             シートには自動で戻しません。SALES_KPI_HUBSPOT_DIRECT を外すとシートを読みます"
                .to_string(),
        ));
    };
    let state = cq_state()?;
    direct.touch();
    if refresh {
        // シートの側も読み直す(従来の ?refresh=1 と同じ)。HubSpot の側は最短間隔を守って背景で更新する
        for name in [
            super::SHEET_SHODAN,
            super::SHEET_APO,
            super::SHEET_CYOMI,
            SHEET_KADEN,
            super::SHEET_KADEN_LIST,
            SHEET_KADEN_BY_OWNER,
            SHEET_MEMBER,
            SHEET_META,
            SHEET_WEEKLY,
            SHEET_KETTEI,
            SHEET_LIST_STOCK,
            SHEET_EXCLUDE,
        ] {
            state.store.invalidate(Some(name)).await;
        }
        direct.request_refresh();
    }
    let snap = direct.snapshot();
    if let Snapshot::NoValue(e) = &snap {
        return Err(CqError::hubspot_direct(
            "hubspot_direct_unavailable",
            format!(
                "HubSpot からまだ 1 度も取れていません(最終失敗 {} / {})。\
                 シートには自動で戻しません",
                e.kind, e.at
            ),
        ));
    }
    let sheets = assemble(&state.client, &state.store, &snap)
        .await
        .map_err(|e| CqError::from_anyhow("sales-kpi", e))?;
    let today = Utc::now()
        .with_timezone(&FixedOffset::east_opt(9 * 3600).expect("JST"))
        .date_naive();
    Ok(Json(super::routes::build_payload(&sheets, today)).into_response())
}
