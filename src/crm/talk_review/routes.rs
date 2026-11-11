//! `/api/crm/talk-review/*` (Google ログインした社内の人なら誰でも。役割は問わない。パスワードログインは拒否)。
//! 認可で落ちる限り HubSpot・Zoom・Turso は呼ばない。

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, SecondsFormat, Utc};
use serde::Deserialize;
use tokio::sync::Semaphore;
use tower_sessions::Session;

use super::checks::{defs, loaded};
use super::config::TalkReviewConfig;
use super::contact::apply_contact;
use super::contact_store;
use super::counts::{CountsCache, TeamOwners};
use super::dials::{self, Dials};
use super::store::{self, ReviewRow, ST_FAILED, ST_JUDGED, ST_PENDING, ST_QUEUED, ST_UNAVAILABLE};
use super::summary::{build, Member, ALERT_RULES};
use super::types::*;
use super::worker::team_members;
use super::zoom::{TranscriptSource, ZoomClient, ZoomError};
use crate::crm::caller_metrics::jst_day;
use crate::crm::rbac;
use crate::crm::routes::{error_json, hubspot_error_response, CrmCtx, CRM_REQUEST_DEADLINE};
use crate::db::turso_http::TursoDb;
use crate::hubspot::deep_link::hubspot_portal_id;
use crate::AppState;

/// アポ数の内訳の待ちの上限 (62 日の全体で HubSpot を約 110 回読む。画面の待ちは 30 秒)
const APO_BREAKDOWN_DEADLINE: Duration = Duration::from_secs(28);
/// 期間の最大日数
pub const MAX_PERIOD_DAYS: i64 = 62;
const ROWS_TTL: Duration = Duration::from_secs(60);
/// 期間ごとの行を持っておく数の上限。1 組が数 MB から数十 MB になりうるので、少なく持つ
const ROWS_CACHE_MAX: usize = 6;
/// Turso から行・印を読む処理を同時に走らせる本数。ページに分けても 1 本で数 MB から数十 MB を一時的に使うので絞る
/// (2026-11-04 に、期間の行を全部の列で読む summary が重なってメモリの上限 2GB を超えて落ちた)
const READ_PERMITS: usize = 2;
const TRANSCRIPT_TTL: Duration = Duration::from_secs(600);
const TRANSCRIPT_MAX: usize = 50;
/// HubSpot の通話の object type id
const CALL_TYPE_ID: &str = "0-48";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeriodError {
    Format,
    Order,
    TooLong,
    Future,
}

/// `from` / `to` (YYYY-MM-DD、省略は今日) を検証する。両端を含み、最大 62 日。`to` が未来なら今日に切り詰める
pub fn parse_period(
    from: Option<&str>,
    to: Option<&str>,
    today: NaiveDate,
) -> Result<(NaiveDate, NaiveDate), PeriodError> {
    let p = |s: Option<&str>| -> Result<NaiveDate, PeriodError> {
        match s.map(str::trim).filter(|s| !s.is_empty()) {
            None => Ok(today),
            Some(t) => NaiveDate::parse_from_str(t, "%Y-%m-%d").map_err(|_| PeriodError::Format),
        }
    };
    let (f, t) = (p(from)?, p(to)?);
    if f > t {
        return Err(PeriodError::Order);
    }
    // 開始が未来なら誤り。終わりが未来なら今日までに切り詰める
    if f > today {
        return Err(PeriodError::Future);
    }
    let t = t.min(today);
    if (t - f).num_days() + 1 > MAX_PERIOD_DAYS {
        return Err(PeriodError::TooLong);
    }
    Ok((f, t))
}

fn period_error(e: PeriodError) -> Response {
    let _ = e;
    error_json(StatusCode::BAD_REQUEST, "invalid_period")
}

/// JST の日 `[from, to]` → UTC の `[開始, 終了)` 文字列
fn utc_bounds(from: NaiveDate, to: NaiveDate) -> (String, String) {
    let s = |d: NaiveDate| {
        store::ts_text(
            DateTime::<Utc>::from_naive_utc_and_offset(d.and_hms_opt(0, 0, 0).unwrap(), Utc)
                - ChronoDuration::hours(9),
        )
    };
    (s(from), s(to + ChronoDuration::days(1)))
}

/// 通話のステータスを API の語に直す
pub fn call_status(db: &str) -> &'static str {
    match db {
        ST_JUDGED => "judged",
        ST_PENDING => "waiting",
        ST_FAILED => "failed",
        ST_UNAVAILABLE => "unavailable",
        _ => "waiting",
    }
}

pub fn call_row(r: &ReviewRow, check_ids: &[String]) -> TalkCallRow {
    let judged = r.status == ST_JUDGED;
    TalkCallRow {
        hubspot_call_id: r.hubspot_call_id.clone(),
        call_at: r.call_at.clone(),
        duration_sec: r.duration_sec,
        status: call_status(&r.status).to_string(),
        outcome: r.outcome.clone().filter(|_| judged),
        items: if judged {
            check_ids
                .iter()
                .filter(|id| {
                    // ランクのためだけに判定した電話は、ヒアリングの項目だけ出す (9 項目は集計に入れていない)
                    !super::apo_call::only(r)
                        || super::checks::loaded()
                            .checks
                            .checks
                            .iter()
                            .any(|c| &c.id == *id && super::judge::is_hearing(c))
                })
                .map(|id| {
                    let item = r.items.iter().find(|i| &i.id == id);
                    TalkItemMark {
                        id: id.clone(),
                        status: item.map_or("na", |i| i.status.as_str()).to_string(),
                        answer: item.and_then(|i| i.answer.clone()),
                    }
                })
                .collect()
        } else {
            vec![]
        },
        summary: r.summary.clone().filter(|_| judged),
        apo_rank: if judged {
            super::apo_rank::rank(&r.items).map(str::to_string)
        } else {
            None
        },
    }
}

// ---------------------------------------------------------------------------
// プロセス内の状態
// ---------------------------------------------------------------------------

type TranscriptCache = Mutex<HashMap<String, (Instant, Result<Vec<TalkTurn>, ZoomError>)>>;

/// 期間 (UTC の `[開始, 終了)`)
type RowsKey = (String, String);
type RowsEntries = HashMap<RowsKey, (Instant, Arc<Vec<ReviewRow>>)>;

/// 期間ごとの、接触した電話の行 (集計用の軽い行)。持つ数と期限を絞り、同じ期間の読み込みは 1 本にする
#[derive(Default)]
struct RowsCache {
    entries: Mutex<RowsEntries>,
    /// 読み込み中の期間の順番待ち
    flights: Mutex<HashMap<RowsKey, Arc<tokio::sync::Mutex<()>>>>,
}

/// 期限が切れた期間を消す
fn drop_expired(g: &mut RowsEntries, now: Instant) {
    g.retain(|_, (at, _)| now.saturating_duration_since(*at) < ROWS_TTL);
}

impl RowsCache {
    /// 期限内の行。期限が切れたものは、読むときにも消す
    fn get(&self, key: &RowsKey, now: Instant) -> Option<Arc<Vec<ReviewRow>>> {
        let mut g = self.entries.lock().unwrap();
        drop_expired(&mut g, now);
        g.get(key).map(|(_, v)| v.clone())
    }

    /// 入れる。期限が切れたものを消し、上限を超えるぶんは古いものから捨てる
    fn put(&self, key: RowsKey, rows: Arc<Vec<ReviewRow>>, now: Instant) {
        let mut g = self.entries.lock().unwrap();
        drop_expired(&mut g, now);
        g.insert(key, (now, rows));
        while g.len() > ROWS_CACHE_MAX {
            let Some(oldest) = g
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            g.remove(&oldest);
        }
    }

    /// 期間の読み込みの順番を取る。持っている間、同じ期間の読み込みは他の要求が待つ
    async fn flight(&self, key: &RowsKey) -> RowsFlight<'_> {
        let lock = self
            .flights
            .lock()
            .unwrap()
            .entry(key.clone())
            .or_default()
            .clone();
        // 待っている途中で要求が切れても、後始末 (Drop) が走るよう、待つ前に作っておく
        let mut flight = RowsFlight {
            cache: self,
            key: key.clone(),
            lock,
            guard: None,
        };
        flight.guard = Some(flight.lock.clone().lock_owned().await);
        flight
    }

    /// 期間の行。無ければ `load` で読む。同じ期間の読み込みは同時に 1 本だけで、順番を待つ間に先の要求が
    /// 読み終えていれば、その結果を使う。読み込みに失敗した結果は持たない
    async fn get_or_load<E, Fut>(
        &self,
        key: RowsKey,
        load: impl FnOnce() -> Fut,
    ) -> Result<Arc<Vec<ReviewRow>>, E>
    where
        Fut: Future<Output = Result<Vec<ReviewRow>, E>>,
    {
        if let Some(v) = self.get(&key, Instant::now()) {
            return Ok(v);
        }
        let _flight = self.flight(&key).await;
        if let Some(v) = self.get(&key, Instant::now()) {
            return Ok(v);
        }
        let rows = Arc::new(load().await?);
        self.put(key, rows.clone(), Instant::now());
        Ok(rows)
    }
}

/// 期間の読み込みの順番。手放すとき、待っている要求がいなければ順番待ちの表から消す
struct RowsFlight<'a> {
    cache: &'a RowsCache,
    key: RowsKey,
    lock: Arc<tokio::sync::Mutex<()>>,
    guard: Option<tokio::sync::OwnedMutexGuard<()>>,
}

impl Drop for RowsFlight<'_> {
    fn drop(&mut self) {
        drop(self.guard.take());
        let mut g = self.cache.flights.lock().unwrap();
        // 表と自分だけが持っているなら、待っている要求はいない (待つ要求は表から取り出すときに数が増える)
        if Arc::strong_count(&self.lock) <= 2 {
            g.remove(&self.key);
        }
    }
}

/// Turso から行・印を読む処理の同時数を絞る枠
struct ReadPermits(Arc<Semaphore>);

impl Default for ReadPermits {
    fn default() -> Self {
        Self(Arc::new(Semaphore::new(READ_PERMITS)))
    }
}

/// 印を読む期間。印の時刻は Zoom の通話ログ、行の時刻は HubSpot の Call なので、境目の電話を取りこぼさないよう
/// 前後に少し広げて読む
pub(super) fn marks_window(from: &str, to: &str) -> (String, String) {
    let pad = chrono::Duration::milliseconds(super::zoom::CALLER_WINDOW_MS);
    let widen = |s: &str, d: chrono::Duration| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|t| store::ts_text(t.with_timezone(&chrono::Utc) + d))
            .unwrap_or_else(|_| s.to_string())
    };
    (widen(from, -pad), widen(to, pad))
}

/// 期間の行のうち、接触した電話 (接触の判定で `contacted = 1`) だけ。集計も一覧もこの行で数える。
/// 行は集計と一覧が使う列だけで読む (`with_summary` は一覧の「一番直すとよい点」用)。
/// `owner` を指定すると、その人の行と印だけを読む。
/// 接触の印が読めなければ全体を失敗にする (定義の違う数を混ぜて出さない)。
/// Turso の同期呼び出しなので、`store::blocking` の中で使う
pub(super) fn read_contacted(
    t: &TursoDb,
    from: &str,
    to: &str,
    owner: Option<&str>,
    with_summary: bool,
) -> Result<Vec<ReviewRow>, String> {
    let rows = store::rows_light_in(t, from, to, owner, with_summary)?;
    if rows.is_empty() {
        return Ok(rows);
    }
    let (mfrom, mto) = marks_window(from, to);
    // 1 人の一覧は、他の人の印と結び付かない (結び付けは同じ担当者の印だけ)
    let marks = match owner {
        Some(o) => contact_store::contacted_marks_for(t, &mfrom, &mto, o)?,
        None => contact_store::contacted_marks(t, &mfrom, &mto)?,
    };
    Ok(super::target::contacted_rows(rows, &marks))
}

/// 変更前の読み方: 行は全部の列を 1 回で、印は全員分を 1 回で読む。軽い読み方・ページ読みの答えの基準と、
/// メモリ計測の「前」に使う (テスト専用)
#[cfg(test)]
pub(super) fn read_contacted_before(
    t: &TursoDb,
    from: &str,
    to: &str,
    owner: Option<&str>,
) -> Result<Vec<ReviewRow>, String> {
    let rows = store::rows_in_single_query(t, from, to, owner)?;
    if rows.is_empty() {
        return Ok(rows);
    }
    let (mfrom, mto) = marks_window(from, to);
    let marks = contact_store::contacted_marks_single_query(t, &mfrom, &mto)?;
    Ok(super::target::contacted_rows(rows, &marks))
}

#[derive(Default)]
pub struct TalkState {
    pub(super) counts: CountsCache,
    pub(super) apo_calls: super::apo_call::ApoCalls,
    apo_breakdown: Arc<super::apo_breakdown::BreakdownCache>,
    rows: RowsCache,
    reads: ReadPermits,
    transcripts: TranscriptCache,
    zoom: OnceLock<Option<Arc<ZoomClient>>>,
}

pub fn shared() -> Arc<TalkState> {
    static S: OnceLock<Arc<TalkState>> = OnceLock::new();
    S.get_or_init(|| Arc::new(TalkState::default())).clone()
}

impl TalkState {
    /// 接触した電話の行を Turso から読む。同時に読むのは `READ_PERMITS` 本までで、枠が空くのを待つ
    async fn load_contacted(
        &self,
        audit: &crate::audit::AuditDb,
        (from, to): (String, String),
        owner: Option<String>,
        with_summary: bool,
    ) -> Result<Vec<ReviewRow>, String> {
        let permit = self
            .reads
            .0
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        store::blocking(audit, move |t| {
            // 読み終わるまで枠を持つ (待っていた要求が途中で切れても、読んでいる間は次の読み込みを始めない)
            let _permit = permit;
            read_contacted(t, &from, &to, owner.as_deref(), with_summary)
        })
        .await
    }

    /// 期間の行 (集計用)。60 秒持ち、同じ期間の読み込みは 1 本だけにする
    // Axum の応答をそのまま認可・読み取りエラーとして返す。
    #[allow(clippy::result_large_err)]
    async fn rows_in(
        &self,
        state: &AppState,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Arc<Vec<ReviewRow>>, Response> {
        let Some(audit) = state.audit.clone() else {
            return Err(error_json(
                StatusCode::SERVICE_UNAVAILABLE,
                "not_configured",
            ));
        };
        let (a, b) = utc_bounds(from, to);
        self.rows
            .get_or_load((a.clone(), b.clone()), || {
                self.load_contacted(&audit, (a, b), None, false)
            })
            .await
            .map_err(|_| error_json(StatusCode::SERVICE_UNAVAILABLE, "review_unavailable"))
    }

    pub(super) fn zoom_client(&self) -> Option<Arc<ZoomClient>> {
        self.zoom.get_or_init(ZoomClient::from_env).clone()
    }

    /// 通話の文字起こし (プロセス内に 10 分持つ。鍵は HubSpot の通話 ID)
    async fn transcript(
        &self,
        hubspot: Option<&crate::hubspot::HubSpotClient>,
        call_id: &str,
        zoom_id: Option<&str>,
    ) -> Result<Vec<TalkTurn>, ZoomError> {
        if let Some((at, v)) = self.transcripts.lock().unwrap().get(call_id) {
            if at.elapsed() < TRANSCRIPT_TTL {
                return v.clone();
            }
        }
        let zoom = self
            .zoom
            .get_or_init(ZoomClient::from_env)
            .clone()
            .ok_or(ZoomError::NotConfigured)?;
        let r = match hubspot {
            Some(h) => super::hs_store::fetch_stored(h, &*zoom, call_id, zoom_id).await,
            None => zoom.fetch_call(call_id, zoom_id).await,
        };
        if cacheable(&r) {
            let mut g = self.transcripts.lock().unwrap();
            if g.len() >= TRANSCRIPT_MAX {
                g.clear();
            }
            g.insert(call_id.to_string(), (Instant::now(), r.clone()));
        }
        r
    }
}

/// 文字起こしの結果を持っておくか。成功と、録音が消えた (Gone) ことだけ。
/// 通信失敗・混雑・まだできていない・設定なしは持たない (次に開いたときにすぐ取り直せるように)。
pub fn cacheable(r: &Result<Vec<TalkTurn>, ZoomError>) -> bool {
    matches!(r, Ok(_) | Err(ZoomError::Gone | ZoomError::Untranscribable))
}

// ---------------------------------------------------------------------------
// ハンドラ
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)]
async fn login_gate(
    session: &Session,
    state: &Arc<AppState>,
    ctx: &CrmCtx,
) -> Result<(), Response> {
    // authorize が Google ログイン・会社ドメイン・許可リスト・無効化アカウントを確かめる。役割は問わない
    rbac::authorize(session, state, &ctx.access, None)
        .await
        .map_err(IntoResponse::into_response)?;
    Ok(())
}

#[derive(Debug, Deserialize, Default)]
pub struct PeriodQuery {
    from: Option<String>,
    to: Option<String>,
    team: Option<String>,
    owner: Option<String>,
}

fn check_ids() -> Vec<String> {
    loaded()
        .checks
        .checks
        .iter()
        .map(|c| c.id.clone())
        .collect()
}

fn now_text(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[allow(clippy::result_large_err)]
async fn owners_list(
    ctx: &CrmCtx,
    client: &crate::hubspot::HubSpotClient,
) -> Result<Arc<crate::crm::owners::CrmOwnersResponse>, Response> {
    match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.queue.owner_list.get(client)).await {
        Err(_) => Err(crate::crm::routes::timeout_response()),
        Ok(Err(e)) => Err(hubspot_error_response(&e)),
        Ok(Ok(l)) => Ok(l),
    }
}

/// 人が属するチーム名 (重複なし、出てきた順)
fn distinct_teams(members: &[Member]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in members.iter().filter_map(|m| m.team.as_ref()) {
        if !out.contains(t) {
            out.push(t.clone());
        }
    }
    out
}

pub(in crate::crm) async fn get_summary(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Query(q): Query<PeriodQuery>,
) -> Response {
    if let Err(r) = login_gate(&session, &state, &ctx).await {
        return r;
    }
    let now = ctx.queue.now();
    let (today, _, _) = jst_day(now);
    let (from, to) = match parse_period(q.from.as_deref(), q.to.as_deref(), today) {
        Ok(v) => v,
        Err(e) => return period_error(e),
    };
    let cfg = TalkReviewConfig::from_env();
    let ids = check_ids();
    let empty_total = |name: &str| TalkPersonRow {
        owner_id: None,
        name: name.to_string(),
        team: None,
        outbound: None,
        dial_breakdown: None,
        dials: None,
        appointments: 0,
        appointment_rate_pct: None,
        appointment_rate_flagged: false,
        contacts: None,
        contact_rate_pct: None,
        date_proposal_calls: None,
        date_proposal_rate_pct: None,
        proposals_per_contact: None,
        contact_pending: 0,
        eligible: 0,
        reviewed: 0,
        waiting: 0,
        unavailable: 0,
        items: ids
            .iter()
            .map(|id| TalkItemRate {
                id: id.clone(),
                not_done: 0,
                applicable: 0,
                rate_pct: None,
                flagged: false,
            })
            .collect(),
        apo_ranks: Default::default(),
    };
    let base = |enabled: bool,
                people: Vec<TalkPersonRow>,
                total: TalkPersonRow,
                team_totals: Vec<TalkPersonRow>,
                counting: &[NaiveDate],
                dials: Option<&Dials>,
                breakdown_pending: bool| TalkSummaryResponse {
        from: from.to_string(),
        to: to.to_string(),
        as_of: now_text(now),
        enabled,
        teams: cfg.teams.clone(),
        checks: defs(),
        rules: ALERT_RULES,
        people,
        per_person_day: cfg.per_person_day,
        counting_days: counting.iter().map(NaiveDate::to_string).collect(),
        // 架電数を出せるのは、enabled でシートが読めたとき
        dials_error: enabled && dials.is_none(),
        dials_partial: dials.is_some_and(|d| d.partial),
        dials_fetched_at: dials.and_then(|d| d.fetched_at.clone()),
        dial_breakdown_pending: breakdown_pending,
        total,
        team_totals,
    };
    if !cfg.enabled || state.audit.is_none() {
        return Json(base(
            false,
            vec![],
            empty_total("この表の全体"),
            vec![],
            &[],
            None,
            false,
        ))
        .into_response();
    }
    let Some(client) = state.crm_hubspot() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let list = match owners_list(&ctx, &client).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let all_members = team_members(&list.owners, &cfg.teams);
    let team = TeamOwners {
        bpo: all_members
            .iter()
            .filter(|m| TalkReviewConfig::is_bpo_team(&m.team))
            .map(|m| m.id.clone())
            .collect(),
        sales: all_members
            .iter()
            .filter(|m| !TalkReviewConfig::is_bpo_team(&m.team))
            .map(|m| m.id.clone())
            .collect(),
    };
    let st = shared();
    let rows = match st.rows_in(&state, from, to).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    // アポ数は、worker が前もって数えた日だけで答える (HubSpot は待たない)。
    // 数え終わっていない日があれば、背景で読み始め (他が読んでいる最中なら任せる)、その日を応答に添える
    let (apo_deals, counting) = st.counts.deals_in(from, to);
    // 直近より前の期間でも、worker がアポを取った電話とランクを決める (再起動で消えるので作り直す)
    st.apo_calls.want(from);
    if !counting.is_empty() {
        let (st, client, team) = (st.clone(), client.clone(), team.clone());
        tokio::spawn(async move {
            if let Err(e) = st.counts.fill(&client, &team, from, to, now, false).await {
                tracing::warn!(error = %e.error_kind(), "talk review: counting failed");
            }
        });
    }
    let deals: Vec<_> = apo_deals.iter().map(|d| d.who.clone()).collect();
    // ランクはアポ数と同じ取引ごと (worker が決めたものを使う。Turso・HubSpot は読まない)
    let deal_ranks = st.apo_calls.ranks();
    // 架電数は営業KPI と同じシート。読めなければ出さない (HubSpot では数えない)
    // 期間に今日が入っていれば、今日の分は Zoom から約 15 分おきに数えた値 (数えられなければシート)
    let live = if from <= today && today <= to {
        match st.zoom_client() {
            Some(z) => {
                super::live_dials::live_today(
                    z,
                    super::live_dials::owner_by_email(&list.owners),
                    today,
                )
                .await
            }
            None => None,
        }
    } else {
        None
    };
    let dials = dials::load_with_live(from, to, today, live.as_deref()).await;
    // 発信の内訳 (発信数のホバー用)。今日は live と同じページから、前の日は背景で取り込んだキャッシュから
    let breakdown = super::dial_breakdown::resolve(
        st.zoom_client(),
        super::live_dials::owner_by_email(&list.owners),
        from,
        to,
        today,
        live.as_deref(),
    );
    let breakdown_pending = breakdown == super::dial_breakdown::Outcome::Pending;
    let members: Vec<Member> = all_members
        .into_iter()
        .filter(|m| {
            q.team
                .as_deref()
                .filter(|t| !t.is_empty())
                .is_none_or(|t| t == m.team)
        })
        .map(|m| Member {
            owner_id: m.id,
            name: m.name,
            team: Some(m.team),
        })
        .collect();
    let (mut people, mut total) = build(
        &members,
        dials.as_ref().map(|d| &d.by_owner),
        dials.as_ref().map(|d| &d.calls_by_owner),
        &deals,
        &rows,
        &ids,
        &ALERT_RULES,
    );
    super::summary::apply_deal_ranks(&mut people, &mut total, &apo_deals, &deal_ranks);
    // 接触・日程切り。読めなければ空欄にする (ほかの列は出す)
    let marks = match state.audit.clone() {
        Some(audit) => {
            let (a, b) = utc_bounds(from, to);
            match store::blocking(&audit, move |t| contact_store::aggregate(t, &a, &b)).await {
                Ok(m) => Some(m),
                Err(e) => {
                    tracing::warn!(error = %e, "talk review: contact marks read failed");
                    None
                }
            }
        }
        None => None,
    };
    apply_contact(&mut people, &mut total, marks.as_ref());
    if let super::dial_breakdown::Outcome::Ready(map) = &breakdown {
        super::dial_breakdown::apply(&mut people, &mut total, map);
    }
    // チームごとの全体の行。表の全体の行と同じ数え方 (その チーム の人だけで build して同じ後処理を通す)
    let mut team_totals: Vec<TalkPersonRow> = Vec::new();
    for name in distinct_teams(&members) {
        let subset: Vec<Member> = members
            .iter()
            .filter(|m| m.team.as_deref() == Some(name.as_str()))
            .cloned()
            .collect();
        let (mut tp, mut tt) = build(
            &subset,
            dials.as_ref().map(|d| &d.by_owner),
            dials.as_ref().map(|d| &d.calls_by_owner),
            &deals,
            &rows,
            &ids,
            &ALERT_RULES,
        );
        super::summary::apply_deal_ranks(&mut tp, &mut tt, &apo_deals, &deal_ranks);
        apply_contact(&mut tp, &mut tt, marks.as_ref());
        if let super::dial_breakdown::Outcome::Ready(map) = &breakdown {
            super::dial_breakdown::apply(&mut tp, &mut tt, map);
        }
        tt.name = name.clone();
        tt.team = Some(name);
        team_totals.push(tt);
    }
    Json(base(
        true,
        people,
        total,
        team_totals,
        &counting,
        dials.as_ref(),
        breakdown_pending,
    ))
    .into_response()
}

pub(in crate::crm) async fn get_calls(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Query(q): Query<PeriodQuery>,
) -> Response {
    if let Err(r) = login_gate(&session, &state, &ctx).await {
        return r;
    }
    let now = ctx.queue.now();
    let (from, to) = match parse_period(q.from.as_deref(), q.to.as_deref(), jst_day(now).0) {
        Ok(v) => v,
        Err(e) => return period_error(e),
    };
    let Some(owner) = q
        .owner
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit()))
    else {
        return error_json(StatusCode::BAD_REQUEST, "invalid_owner");
    };
    let owner = owner.to_string();
    let Some(audit) = state
        .audit
        .clone()
        .filter(|_| TalkReviewConfig::from_env().enabled)
    else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    // 1 人分の行と印だけを、summary と同じ同時数の枠の中で読む。一覧は「一番直すとよい点」(summary) も使う
    let rows = match shared()
        .load_contacted(&audit, utc_bounds(from, to), Some(owner.clone()), true)
        .await
    {
        Ok(r) => r,
        Err(_) => return error_json(StatusCode::SERVICE_UNAVAILABLE, "review_unavailable"),
    };
    // 名前は付け足し。取れなければ空の代わりの文を出す
    let name = match state.crm_hubspot() {
        Some(c) => {
            match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.queue.owner_list.get(&c)).await {
                Ok(Ok(l)) => l
                    .owners
                    .iter()
                    .find(|o| o.id == owner)
                    .map(|o| o.name.clone()),
                _ => None,
            }
        }
        None => None,
    }
    .unwrap_or_else(|| "(名前を取得できません)".to_string());
    let ids = check_ids();
    Json(TalkCallsResponse {
        owner_id: owner,
        name,
        from: from.to_string(),
        to: to.to_string(),
        checks: defs(),
        // 枠待ちで判定しない電話は一覧に出さない
        calls: rows
            .iter()
            .filter(|r| r.status != ST_QUEUED)
            .map(|r| call_row(r, &ids))
            .collect(),
    })
    .into_response()
}

/// アポ数の内訳 (アポ数を押すと開く)。`owner` があればその人、無ければ表の全体 (`team` で絞った人)。
/// アポを数え終わっていない日があれば 503 `counting` (画面は数秒おきに取り直す)
pub(in crate::crm) async fn get_appointments(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Query(q): Query<PeriodQuery>,
) -> Response {
    if let Err(r) = login_gate(&session, &state, &ctx).await {
        return r;
    }
    let now = ctx.queue.now();
    let (today, _, _) = jst_day(now);
    let (from, to) = match parse_period(q.from.as_deref(), q.to.as_deref(), today) {
        Ok(v) => v,
        Err(e) => return period_error(e),
    };
    let owner: Option<String> = match q.owner.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(s) if s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit()) => Some(s.to_string()),
        Some(_) => return error_json(StatusCode::BAD_REQUEST, "invalid_owner"),
    };
    let cfg = TalkReviewConfig::from_env();
    if !cfg.enabled || state.audit.is_none() {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    }
    let Some(client) = state.crm_hubspot() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let list = match owners_list(&ctx, &client).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let all_members = team_members(&list.owners, &cfg.teams);
    let st = shared();
    let (deals, counting) = st.counts.deals_in(from, to);
    if !counting.is_empty() {
        let team = TeamOwners {
            bpo: all_members
                .iter()
                .filter(|m| TalkReviewConfig::is_bpo_team(&m.team))
                .map(|m| m.id.clone())
                .collect(),
            sales: all_members
                .iter()
                .filter(|m| !TalkReviewConfig::is_bpo_team(&m.team))
                .map(|m| m.id.clone())
                .collect(),
        };
        let (st, client) = (st.clone(), client.clone());
        tokio::spawn(async move {
            if let Err(e) = st.counts.fill(&client, &team, from, to, now, false).await {
                tracing::warn!(error = %e.error_kind(), "talk review: counting failed");
            }
        });
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "counting");
    }
    // 対象の人: 1 人、または表の全体 (チームで絞った人。summary の全体の行と同じ顔ぶれ)
    let who: BTreeSet<String> = match &owner {
        Some(o) => [o.clone()].into_iter().collect(),
        None => all_members
            .iter()
            .filter(|m| {
                q.team
                    .as_deref()
                    .filter(|t| !t.is_empty())
                    .is_none_or(|t| t == m.team)
            })
            .map(|m| m.id.clone())
            .collect(),
    };
    let mine: Vec<super::counts::ApoDeal> = deals
        .into_iter()
        .filter(|d| d.who.iter().any(|w| who.contains(w)))
        .collect();
    let key = super::apo_breakdown::cache_key(&mine);
    let ttl = if to >= today - ChronoDuration::days(1) {
        super::apo_breakdown::RECENT_TTL
    } else {
        super::apo_breakdown::SETTLED_TTL
    };
    let portal = hubspot_portal_id();
    let bg = client.background();
    let load = async move { super::apo_breakdown::breakdown(&bg, &portal, &mine).await };
    let rows = match st
        .apo_breakdown
        .get_or_load(key, ttl, APO_BREAKDOWN_DEADLINE, load)
        .await
    {
        Ok(v) => v,
        Err(super::apo_breakdown::LoadError::Timeout) => {
            return crate::crm::routes::timeout_response()
        }
        Err(super::apo_breakdown::LoadError::Hub(e)) => return hubspot_error_response(&e),
        Err(super::apo_breakdown::LoadError::Aborted) => {
            return error_json(StatusCode::SERVICE_UNAVAILABLE, "aborted")
        }
    };
    let name = match &owner {
        Some(o) => list
            .owners
            .iter()
            .find(|x| &x.id == o)
            .map(|x| x.name.clone())
            .unwrap_or_else(|| "(名前を取得できません)".to_string()),
        None => "この表の全体".to_string(),
    };
    Json(TalkApoResponse {
        owner_id: owner,
        name,
        from: from.to_string(),
        to: to.to_string(),
        counts: super::apo_breakdown::counts_of(&rows),
        deals: rows,
    })
    .into_response()
}

pub fn transcript_error_text(e: &ZoomError) -> &'static str {
    match e {
        ZoomError::Gone => "録音が残っていません",
        ZoomError::Untranscribable => "短すぎる通話か、設定により文字起こしできません",
        ZoomError::NotReady => "文字起こしがまだできていません",
        ZoomError::Transient(_) => {
            "Zoom に接続できませんでした。少し待ってからもう一度開いてください"
        }
        ZoomError::NotConfigured => "Zoom の設定がないため文字起こしを表示できません",
    }
}

pub(in crate::crm) async fn get_call_detail(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
) -> Response {
    if let Err(r) = login_gate(&session, &state, &ctx).await {
        return r;
    }
    if id.is_empty() || id.len() > 20 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return error_json(StatusCode::BAD_REQUEST, "invalid_call");
    }
    let Some(audit) = state
        .audit
        .clone()
        .filter(|_| TalkReviewConfig::from_env().enabled)
    else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let id2 = id.clone();
    let row = match store::blocking(&audit, move |t| store::get(t, &id2)).await {
        Ok(Some(r)) => r,
        Ok(None) => return error_json(StatusCode::NOT_FOUND, "not_found"),
        Err(_) => return error_json(StatusCode::SERVICE_UNAVAILABLE, "review_unavailable"),
    };
    let rep_name = match state.crm_hubspot() {
        Some(c) => {
            match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.queue.owner_list.get(&c)).await {
                Ok(Ok(l)) => l
                    .owners
                    .iter()
                    .find(|o| o.id == row.owner_id)
                    .map(|o| o.name.clone()),
                _ => None,
            }
        }
        None => None,
    }
    .unwrap_or_else(|| "(名前を取得できません)".to_string());
    // HubSpot の通話に残した文字起こしを先に読み、無ければ Zoom から取って残す
    let hubspot = state.crm_hubspot();
    let st = shared();
    let fetched = st.transcript(
        hubspot.as_deref(),
        &row.hubspot_call_id,
        row.zoom_call_log_id.as_deref(),
    );
    let (transcript, transcript_error) =
        match tokio::time::timeout(CRM_REQUEST_DEADLINE, fetched).await {
            Ok(Ok(t)) => (Some(t), None),
            Ok(Err(ZoomError::Gone)) if row.zoom_call_log_id.is_none() => {
                (None, Some("録音が残っていません".to_string()))
            }
            Ok(Err(e)) => (None, Some(transcript_error_text(&e).to_string())),
            Err(_) => (
                None,
                Some(transcript_error_text(&ZoomError::Transient(String::new())).to_string()),
            ),
        };
    let ids = check_ids();
    let judged = row.status == ST_JUDGED;
    let items = loaded()
        .checks
        .checks
        .iter()
        .filter(|c| !super::apo_call::only(&row) || super::judge::is_hearing(c))
        .map(
            |c| match row.items.iter().find(|i| i.id == c.id).filter(|_| judged) {
                Some(i) => TalkItemDetail {
                    id: c.id.clone(),
                    status: i.status.clone(),
                    evidence: i.evidence.clone(),
                    quote: i.quote.clone(),
                    advice: i.advice.clone(),
                    note: i.note.clone(),
                    answer: i.answer.clone(),
                    asked: i.asked,
                    answered: i.answered,
                    counted_from: i.counted_from.clone(),
                },
                None => TalkItemDetail {
                    id: c.id.clone(),
                    status: "na".into(),
                    evidence: vec![],
                    quote: None,
                    advice: None,
                    note: None,
                    answer: None,
                    asked: None,
                    answered: None,
                    counted_from: None,
                },
            },
        )
        .collect();
    Json(TalkCallDetailResponse {
        call: call_row(&row, &ids),
        owner_id: row.owner_id.clone(),
        rep_name,
        checks: defs(),
        items,
        reached_person: row.reached_person.filter(|_| judged),
        reception: row.reception.filter(|_| judged),
        first_date_turn: row.first_date_turn.filter(|_| judged),
        transcript,
        transcript_error,
        hubspot_url: format!(
            "https://app.hubspot.com/contacts/{}/record/{CALL_TYPE_ID}/{}/",
            hubspot_portal_id(),
            row.hubspot_call_id
        ),
        model: row.model.clone(),
        checks_version: row.checks_version.clone(),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::caller_metrics_tests::{inject_session, login, test_state};
    use crate::crm::rbac::{CrmAccess, CrmRole};
    use axum::{
        body::Body,
        http::{header, Request},
        routing::post,
        Router,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;
    use tower_sessions::{MemoryStore, SessionManagerLayer};

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn period_defaults_to_today_and_validates() {
        let today = d("2026-10-19");
        assert_eq!(parse_period(None, None, today), Ok((today, today)));
        assert_eq!(
            parse_period(Some("2026-10-01"), Some("2026-10-07"), today),
            Ok((d("2026-10-01"), d("2026-10-07")))
        );
        assert_eq!(
            parse_period(Some("2026-10-07"), None, today),
            Ok((d("2026-10-07"), today))
        );
        assert_eq!(
            parse_period(Some("2026-10-20"), Some("2026-10-19"), today),
            Err(PeriodError::Order)
        );
        assert_eq!(
            parse_period(Some("10/01"), None, today),
            Err(PeriodError::Format)
        );
        // 62 日は通り、63 日は超過
        assert!(parse_period(Some("2026-08-19"), Some("2026-10-19"), today).is_ok());
        assert_eq!(
            parse_period(Some("2026-08-18"), Some("2026-10-19"), today),
            Err(PeriodError::TooLong)
        );
    }

    #[test]
    fn transcript_cache_keeps_only_success_and_gone() {
        assert!(cacheable(&Ok(vec![])));
        assert!(cacheable(&Err(ZoomError::Gone)));
        assert!(cacheable(&Err(ZoomError::Untranscribable)));
        assert!(!cacheable(&Err(ZoomError::NotReady)));
        assert!(!cacheable(&Err(ZoomError::Transient("http 429".into()))));
        assert!(!cacheable(&Err(ZoomError::NotConfigured)));
    }

    #[test]
    fn jst_bounds_are_nine_hours_earlier_in_utc() {
        let (a, b) = utc_bounds(d("2026-10-19"), d("2026-10-20"));
        assert_eq!(a, "2026-10-18T15:00:00Z");
        assert_eq!(b, "2026-10-20T15:00:00Z");
    }

    #[test]
    fn call_row_marks_follow_checks_order_only_when_judged() {
        use crate::crm::talk_review::judge::StoredItem;
        let mut r = ReviewRow {
            hubspot_call_id: "9".into(),
            owner_id: "1".into(),
            call_at: "2026-10-19T01:00:00Z".into(),
            duration_sec: 80,
            zoom_call_log_id: None,
            status: "judged".into(),
            attempts: 1,
            checks_version: None,
            model: None,
            outcome: Some("決まらなかった".into()),
            reached_person: None,
            reception: None,
            first_date_turn: None,
            items: vec![StoredItem {
                id: "early_date".into(),
                status: "not_done".into(),
                evidence: vec![],
                quote: None,
                advice: None,
                note: None,
                answer: None,
                asked: None,
                answered: None,
                counted_from: None,
            }],
            summary: Some("要約".into()),
            note: None,
        };
        let ids = check_ids();
        let c = call_row(&r, &ids);
        assert_eq!(c.status, "judged");
        assert_eq!(c.items.len(), 15);
        assert_eq!(
            (c.items[2].id.as_str(), c.items[2].status.as_str()),
            ("early_date", "not_done")
        );
        assert_eq!(c.items[0].status, "na");
        assert_eq!(c.items[2].answer, None);
        // ヒアリングの項目は先方の答えも一覧に出す
        r.items.push(StoredItem {
            id: "aq_budget".into(),
            status: "done".into(),
            evidence: vec![],
            quote: None,
            advice: None,
            note: None,
            answer: Some("具体額".into()),
            asked: Some(true),
            answered: Some(true),
            counted_from: None,
        });
        let c = call_row(&r, &ids);
        let b = c.items.iter().find(|x| x.id == "aq_budget").unwrap();
        assert_eq!(
            (b.status.as_str(), b.answer.as_deref()),
            ("done", Some("具体額"))
        );
        r.status = "pending_transcript".into();
        let c = call_row(&r, &ids);
        assert_eq!(c.status, "waiting");
        assert!(c.items.is_empty() && c.outcome.is_none() && c.summary.is_none());
        assert_eq!(call_status("failed"), "failed");
        assert_eq!(call_status("unavailable"), "unavailable");
    }

    /// パスワードログインの人 (社内パスワード) のセッションを作る
    async fn inject_password_session(session: Session, Json(v): Json<Value>) -> StatusCode {
        session
            .insert(crate::auth::SESSION_USER_KEY, v["email"].as_str().unwrap())
            .await
            .unwrap();
        session
            .insert(
                crate::auth::SESSION_LOGIN_METHOD_KEY,
                crate::auth::LOGIN_METHOD_PASSWORD_INTERNAL,
            )
            .await
            .unwrap();
        StatusCode::NO_CONTENT
    }

    async fn password_login(app: &Router, email: &str) -> String {
        let resp = app
            .clone()
            .oneshot(
                Request::post("/__test/pw_session")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"email": email}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        resp.headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .find(|c| c.starts_with("id="))
            .map(|c| c.split(';').next().unwrap().to_string())
            .expect("cookie")
    }

    async fn app() -> (Router, String, String) {
        let admin = "ta@f-a-c.co.jp";
        let bpo = "tb@f-a-c.co.jp";
        let app = Router::new()
            .merge(crate::crm::routes::router(
                CrmAccess::from_list(&format!("{admin},{bpo}"))
                    .with_test_role(admin, CrmRole::Admin)
                    .with_test_role(bpo, CrmRole::Bpo),
            ))
            .route("/__test/session", post(inject_session))
            .route("/__test/pw_session", post(inject_password_session))
            .with_state(test_state(None))
            .layer(SessionManagerLayer::new(MemoryStore::default()));
        let a = login(&app, admin).await;
        let b = login(&app, bpo).await;
        (app, a, b)
    }

    async fn get(app: &Router, uri: &str, cookie: Option<&str>) -> (StatusCode, Value) {
        let mut b = Request::builder().uri(uri);
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        let resp = app
            .clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let st = resp.status();
        let bytes = http_body_util::BodyExt::collect(resp.into_body())
            .await
            .unwrap()
            .to_bytes();
        (st, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn google_login_of_any_role_gets_200_password_403_anonymous_401() {
        let (app, admin, bpo) = app().await;
        let pw = password_login(&app, "ta@f-a-c.co.jp").await;
        for uri in [
            "/api/crm/talk-review/summary",
            "/api/crm/talk-review/calls?owner=1",
            "/api/crm/talk-review/calls/123",
            "/api/crm/talk-review/appointments?owner=1",
        ] {
            // 管理者でない Google ログインの人も認可は通る (中身の検証で 400 になる URL は 403/401 でないことを見る)
            let (s, v) = get(&app, uri, Some(&bpo)).await;
            assert_ne!(s, StatusCode::FORBIDDEN, "{uri} {v}");
            assert_ne!(s, StatusCode::UNAUTHORIZED, "{uri} {v}");
            // パスワードログインは管理者のメールでも拒否
            let (s, v) = get(&app, uri, Some(&pw)).await;
            assert_eq!(
                (s, v["error_kind"].as_str()),
                (StatusCode::FORBIDDEN, Some("google_login_required")),
                "{uri}"
            );
            let (s, _) = get(&app, uri, None).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{uri}");
        }
        // アポの内訳: 担当者 ID の誤りは 400、機能が無効なら 503 not_configured (外部は呼ばない)
        let (s, v) = get(
            &app,
            "/api/crm/talk-review/appointments?owner=abc",
            Some(&bpo),
        )
        .await;
        assert_eq!(
            (s, v["error_kind"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_owner"))
        );
        let (s, v) = get(
            &app,
            "/api/crm/talk-review/appointments?owner=1&from=2026-10-01&to=2026-10-02",
            Some(&bpo),
        )
        .await;
        assert_eq!(
            (s, v["error_kind"].as_str()),
            (StatusCode::SERVICE_UNAVAILABLE, Some("not_configured"))
        );
        // 管理者でない人も summary は 200
        let (s, v) = get(
            &app,
            "/api/crm/talk-review/summary?from=2026-10-01&to=2026-10-02",
            Some(&bpo),
        )
        .await;
        assert_eq!((s, v["enabled"].clone()), (StatusCode::OK, json!(false)));
        // 管理者: 期間の誤りは 400、機能が無効なら中身は空で 200 (外部は呼ばない)
        let (s, v) = get(
            &app,
            "/api/crm/talk-review/summary?from=2026-08-01&to=2026-10-19",
            Some(&admin),
        )
        .await;
        assert_eq!(
            (s, v["error_kind"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_period"))
        );
        let (s, v) = get(
            &app,
            "/api/crm/talk-review/summary?from=2026-10-02&to=2026-10-01",
            Some(&admin),
        )
        .await;
        assert_eq!(
            (s, v["error_kind"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_period"))
        );
        let (s, v) = get(
            &app,
            "/api/crm/talk-review/summary?from=2026-10-01&to=2026-10-02",
            Some(&admin),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["enabled"], json!(false));
        assert_eq!(
            (v["from"].as_str(), v["to"].as_str()),
            (Some("2026-10-01"), Some("2026-10-02"))
        );
        assert_eq!(v["people"], json!([]));
        assert_eq!(v["checks"].as_array().unwrap().len(), 15);
        assert_eq!(v["rules"]["item_min_applicable"], json!(5));
        assert_eq!(v["rules"]["appointment_min_dials"], json!(100));
        assert_eq!(v["total"]["items"].as_array().unwrap().len(), 15);
        let (s, v) = get(
            &app,
            "/api/crm/talk-review/calls?from=2026-10-01&to=2026-10-02",
            Some(&admin),
        )
        .await;
        assert_eq!(
            (s, v["error_kind"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_owner"))
        );
        let (s, _) = get(&app, "/api/crm/talk-review/calls/abc", Some(&admin)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    // ---- 行の読み込み (キャッシュ・同時数・軽い行) ------------------------------------------------

    use crate::audit::fake_turso::{start_sqlite_audit_with_queries, SharedConn};
    use crate::crm::talk_review::judge::StoredItem;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn rk(n: usize) -> RowsKey {
        (
            format!("2026-10-{n:02}T00:00:00Z"),
            format!("2026-10-{n:02}T23:00:00Z"),
        )
    }

    fn test_row(id: &str) -> ReviewRow {
        ReviewRow {
            hubspot_call_id: id.into(),
            owner_id: "1".into(),
            call_at: "2026-10-19T01:00:00Z".into(),
            duration_sec: 80,
            zoom_call_log_id: None,
            status: "judged".into(),
            attempts: 0,
            checks_version: None,
            model: None,
            outcome: None,
            reached_person: None,
            reception: None,
            first_date_turn: None,
            items: vec![],
            summary: None,
            note: None,
        }
    }

    fn rows_of(n: usize) -> Arc<Vec<ReviewRow>> {
        Arc::new((0..n).map(|i| test_row(&format!("r{i}"))).collect())
    }

    fn cached_keys(c: &RowsCache) -> Vec<RowsKey> {
        let mut k: Vec<RowsKey> = c.entries.lock().unwrap().keys().cloned().collect();
        k.sort();
        k
    }

    #[test]
    fn rows_cache_expires_at_the_ttl_and_drops_expired_entries_when_putting() {
        let c = RowsCache::default();
        let t0 = Instant::now();
        c.put(rk(1), rows_of(3), t0);
        // 期限の 1 ミリ秒前までは返し、ちょうど期限で切れる
        let almost = t0 + ROWS_TTL - Duration::from_millis(1);
        assert_eq!(c.get(&rk(1), almost).map(|r| r.len()), Some(3));
        assert!(c.get(&rk(1), t0 + ROWS_TTL).is_none());
        assert!(
            cached_keys(&c).is_empty(),
            "期限が切れた期間は読むときにも消える"
        );

        // 入れるときに、期限が切れた期間を消す (別の期間が来るまで居座らない)
        c.put(rk(1), rows_of(1), t0);
        c.put(rk(2), rows_of(1), t0 + Duration::from_secs(10));
        assert_eq!(cached_keys(&c), vec![rk(1), rk(2)]);
        c.put(rk(3), rows_of(1), t0 + ROWS_TTL + Duration::from_secs(1));
        assert_eq!(
            cached_keys(&c),
            vec![rk(2), rk(3)],
            "61 秒前の 1 は消え、51 秒前の 2 は残る"
        );
    }

    #[test]
    fn rows_cache_keeps_at_most_six_periods_and_drops_the_oldest() {
        assert_eq!(ROWS_CACHE_MAX, 6);
        let c = RowsCache::default();
        let t0 = Instant::now();
        // 期限内に 10 期間を入れる。残るのは新しい 6 期間 (5〜10 日)
        for i in 1..=10 {
            c.put(rk(i), rows_of(i), t0 + Duration::from_secs(i as u64));
        }
        assert_eq!(cached_keys(&c), (5..=10).map(rk).collect::<Vec<_>>());
        let now = t0 + Duration::from_secs(10);
        assert!(c.get(&rk(4), now).is_none());
        assert_eq!(c.get(&rk(5), now).map(|r| r.len()), Some(5));
        assert_eq!(c.get(&rk(10), now).map(|r| r.len()), Some(10));
        // 同じ期間を入れ直しても数は増えない
        c.put(rk(10), rows_of(2), now);
        assert_eq!(cached_keys(&c).len(), 6);
        assert_eq!(c.get(&rk(10), now).map(|r| r.len()), Some(2));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn simultaneous_requests_for_one_period_read_it_only_once() {
        let c = Arc::new(RowsCache::default());
        let loads = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let (c, loads) = (c.clone(), loads.clone());
            tasks.push(tokio::spawn(async move {
                c.get_or_load(rk(1), || async move {
                    loads.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<_, String>(vec![test_row("a"), test_row("b")])
                })
                .await
            }));
        }
        let mut got = Vec::new();
        for t in tasks {
            got.push(t.await.unwrap().unwrap());
        }
        assert_eq!(loads.load(Ordering::SeqCst), 1, "読み込みは 1 本だけ");
        // 待っていた要求は、先に読み終えた結果 (同じ行の集まり) を使う
        assert!(got.iter().all(|r| Arc::ptr_eq(r, &got[0])));
        assert_eq!(got[0].len(), 2);
        assert!(
            c.flights.lock().unwrap().is_empty(),
            "順番待ちの表は空に戻る"
        );
        assert_eq!(cached_keys(&c), vec![rk(1)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn different_periods_are_read_side_by_side() {
        let c = Arc::new(RowsCache::default());
        // 2 つの読み込みが同時に走っていないと、どちらも先へ進めない
        let both = Arc::new(tokio::sync::Barrier::new(2));
        let start = |n: usize| {
            let (c, both) = (c.clone(), both.clone());
            tokio::spawn(async move {
                c.get_or_load(rk(n), || async move {
                    both.wait().await;
                    Ok::<_, String>(vec![test_row("x")])
                })
                .await
            })
        };
        let (a, b) = (start(1), start(2));
        let done = tokio::time::timeout(Duration::from_secs(5), async { (a.await, b.await) }).await;
        let (a, b) = done.expect("別の期間の読み込みが互いを待っている");
        assert_eq!(
            (a.unwrap().unwrap().len(), b.unwrap().unwrap().len()),
            (1, 1)
        );
        assert_eq!(cached_keys(&c), vec![rk(1), rk(2)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_read_is_not_kept_and_the_next_request_reads_again() {
        let c = RowsCache::default();
        let loads = AtomicUsize::new(0);
        let load = |ok: bool| {
            let n = &loads;
            async move {
                n.fetch_add(1, Ordering::SeqCst);
                if ok {
                    Ok(vec![test_row("a")])
                } else {
                    Err("boom".to_string())
                }
            }
        };
        assert_eq!(
            c.get_or_load(rk(1), || load(false)).await.err().as_deref(),
            Some("boom")
        );
        assert!(cached_keys(&c).is_empty());
        assert_eq!(c.get_or_load(rk(1), || load(true)).await.unwrap().len(), 1);
        // 読めたあとは持っているので、もう読まない
        assert_eq!(c.get_or_load(rk(1), || load(false)).await.unwrap().len(), 1);
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert!(c.flights.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_dropped_request_leaves_no_waiting_entry_and_the_next_one_takes_over() {
        let c = Arc::new(RowsCache::default());
        let stuck = {
            let c = c.clone();
            tokio::spawn(async move {
                c.get_or_load(rk(1), || {
                    std::future::pending::<Result<Vec<ReviewRow>, String>>()
                })
                .await
            })
        };
        // 先頭が順番を取るまで待つ
        for _ in 0..200 {
            if !c.flights.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(c.flights.lock().unwrap().len(), 1);
        let waiter = {
            let c = c.clone();
            tokio::spawn(async move {
                c.get_or_load(rk(1), || async { Ok::<_, String>(vec![test_row("w")]) })
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!waiter.is_finished(), "先頭の読み込みが終わるまで待つ");
        // 待っている要求が切れても、先頭の順番は残る
        let gone = {
            let c = c.clone();
            tokio::spawn(async move {
                c.get_or_load(rk(1), || async { Ok::<_, String>(vec![]) })
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        gone.abort();
        let _ = gone.await;
        assert_eq!(c.flights.lock().unwrap().len(), 1);
        // 先頭が切れると、待っていた要求が引き継いで読む
        stuck.abort();
        let _ = stuck.await;
        let rows = tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .expect("先頭が切れても、待っている要求が進まない")
            .unwrap()
            .unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.hubspot_call_id.as_str())
                .collect::<Vec<_>>(),
            vec!["w"]
        );
        assert!(
            c.flights.lock().unwrap().is_empty(),
            "順番待ちの表が残らない"
        );
    }

    const FROM: &str = "2026-10-15T00:00:00Z";
    const TO: &str = "2026-10-17T00:00:00Z";

    fn item(id: &str, status: &str, hearing: bool) -> StoredItem {
        StoredItem {
            id: id.into(),
            status: status.into(),
            evidence: vec![2, 5],
            quote: Some("来週の火曜日でどうですか".into()),
            advice: Some("先に相手の状況を聞いてから日程を出す".into()),
            note: Some("メモ".into()),
            answer: hearing.then(|| "具体額".to_string()),
            asked: hearing.then_some(true),
            answered: hearing.then_some(true),
            counted_from: None,
        }
    }

    /// 15 項目すべての判定。`early_date` だけ指定の判定、ほかは done / not_done / na を順に回す
    fn all_items(early: &str) -> String {
        let defs = defs();
        let items: Vec<StoredItem> = defs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let status = if d.id == "early_date" {
                    early
                } else {
                    ["done", "not_done", "na"][i % 3]
                };
                item(&d.id, status, !d.answers.is_empty())
            })
            .collect();
        serde_json::to_string(&items).unwrap()
    }

    fn put_row(
        conn: &SharedConn,
        id: &str,
        owner: &str,
        at: &str,
        status: &str,
        early: Option<&str>,
    ) {
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO call_talk_reviews (hubspot_call_id, owner_id, call_at, duration_sec, \
                 zoom_call_log_id, status, attempts, checks_version, model, outcome, reached_person, \
                 reception, first_date_turn, items, summary, note, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 95, 'zoom-' || ?1, ?4, 2, 'v1', 'MiniMax-M3', '決まらなかった', 1, 0, 3, \
                 ?5, ?6, 'contacted', 'c', 'u')",
                rusqlite::params![
                    id,
                    owner,
                    at,
                    status,
                    early.map(all_items),
                    format!("{id} のまとめ")
                ],
            )
            .unwrap();
    }

    fn put_mark(conn: &SharedConn, id: &str, owner: &str, at: &str, contacted: i64) {
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO call_contact_marks (zoom_call_id, owner_id, call_at, connected, status, \
                 contacted, date_proposals, attempts, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 1, 'judged', ?4, 0, 0, 'c', 'u')",
                rusqlite::params![id, owner, at, contacted],
            )
            .unwrap();
    }

    /// 期間の行 11 本 (r1〜r11) と印。接触した印と結び付くのは 9 本 (r3 は接触なし、r8 は別の担当者の印)。
    /// r1・r5・r8 は同じ call_at (rowid の大きい順に並ぶ)
    fn seed_calls(conn: &SharedConn) {
        for (id, owner, at, status, early) in [
            (
                "r1",
                "o1",
                "2026-10-15T01:00:00Z",
                "judged",
                Some("not_done"),
            ),
            ("r2", "o1", "2026-10-15T02:00:00Z", "judged", Some("done")),
            ("r3", "o1", "2026-10-15T03:00:00Z", "judged", Some("na")),
            (
                "r4",
                "o1",
                "2026-10-16T01:00:00Z",
                "pending_transcript",
                None,
            ),
            (
                "r5",
                "o2",
                "2026-10-15T01:00:00Z",
                "judged",
                Some("not_done"),
            ),
            ("r6", "o2", "2026-10-15T05:00:00Z", "unavailable", None),
            ("r7", "o2", "2026-10-16T05:00:00Z", "judged", Some("done")),
            (
                "r8",
                "o3",
                "2026-10-15T01:00:00Z",
                "judged",
                Some("not_done"),
            ),
            ("r9", "o1", "2026-10-15T07:00:00Z", "failed", None),
            ("r10", "o3", "2026-10-16T08:00:00Z", "judged", Some("done")),
            ("r11", "o1", "2026-10-16T09:00:00Z", "queued", None),
        ] {
            put_row(conn, id, owner, at, status, early);
        }
        for (id, owner, at, contacted) in [
            ("k1", "o1", "2026-10-15T01:00:10Z", 1),
            ("k2", "o1", "2026-10-15T02:00:00Z", 1),
            ("k3", "o1", "2026-10-15T03:00:00Z", 0),
            ("k4", "o1", "2026-10-16T01:00:30Z", 1),
            ("k5", "o2", "2026-10-15T01:00:20Z", 1),
            ("k6", "o2", "2026-10-15T05:00:00Z", 1),
            ("k7", "o2", "2026-10-16T05:00:00Z", 1),
            ("k8", "o9", "2026-10-15T01:00:00Z", 1),
            ("k9", "o1", "2026-10-15T07:00:00Z", 1),
            ("k10", "o3", "2026-10-16T08:00:00Z", 1),
            ("k11", "o1", "2026-10-16T09:00:00Z", 1),
        ] {
            put_mark(conn, id, owner, at, contacted);
        }
    }

    fn call_ids(rows: &[ReviewRow]) -> Vec<&str> {
        rows.iter().map(|r| r.hubspot_call_id.as_str()).collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_table_and_the_list_are_the_same_from_light_rows_as_from_full_rows() {
        let (audit, conn, _) = start_sqlite_audit_with_queries().await;
        seed_calls(&conn);
        store::blocking(&audit, |t| {
            let ids = check_ids();
            let members = ["o1", "o2", "o3"]
                .iter()
                .map(|o| Member {
                    owner_id: o.to_string(),
                    name: format!("名前{o}"),
                    team: Some("新規営業".into()),
                })
                .collect::<Vec<_>>();
            let dials: HashMap<String, u32> =
                [("o1".into(), 120), ("o2".into(), 120), ("o3".into(), 120)].into();
            let deals: Vec<Vec<String>> = vec![vec!["o1".into()], vec!["o2".into()]];

            let legacy = read_contacted_before(t, FROM, TO, None)?;
            // 接触した印と結び付いた 9 本が、新しい順 (r1・r5・r8 は同じ call_at で rowid の大きい順) に残る
            assert_eq!(
                call_ids(&legacy),
                vec!["r11", "r10", "r7", "r4", "r9", "r6", "r2", "r5", "r1"]
            );
            let light = read_contacted(t, FROM, TO, None, false)?;
            assert_eq!(call_ids(&light), call_ids(&legacy));
            // 集計 (人ごとの表・全体の行) は、全部の列の行から作っても軽い行から作っても同じ
            let args = |rows: &[ReviewRow]| {
                build(
                    &members,
                    Some(&dials),
                    Some(&dials),
                    &deals,
                    rows,
                    &ids,
                    &ALERT_RULES,
                )
            };
            let (people_full, total_full) = args(&legacy);
            let (people_light, total_light) = args(&light);
            assert_eq!(people_light, people_full);
            assert_eq!(total_light, total_full);
            // 具体的な値: 全体の判定済みは 5 本 (r1・r2・r5・r7・r10)、待ち 1 (r4)、取れない 2 (r6・r9)、対象 9
            assert_eq!(
                (
                    total_light.reviewed,
                    total_light.waiting,
                    total_light.unavailable,
                    total_light.eligible
                ),
                (5, 1, 2, 9)
            );
            // early_date のできていない割合は 5 本中 2 本 (r1・r5) = 40%
            let early = |row: &TalkPersonRow| {
                row.items
                    .iter()
                    .find(|i| i.id == "early_date")
                    .unwrap()
                    .clone()
            };
            let e = early(&total_light);
            assert_eq!((e.not_done, e.applicable), (2, 5));
            assert!((e.rate_pct.unwrap() - 40.0).abs() < 1e-9);
            // o1 は判定済み 2 本 (r1 = できていない・r2 = できた) で 50%
            let o1 = people_light
                .iter()
                .find(|p| p.owner_id.as_deref() == Some("o1"))
                .unwrap();
            assert_eq!(
                (o1.reviewed, o1.waiting, o1.unavailable, o1.eligible),
                (2, 1, 1, 5)
            );
            let e = early(o1);
            assert_eq!((e.not_done, e.applicable, e.rate_pct), (1, 2, Some(50.0)));

            // 一覧 (1 人分): 担当者の印だけを読んでも、全員分の印を読んだ基準と同じ行・同じ表示
            for owner in ["o1", "o2", "o3", "nobody"] {
                let legacy = read_contacted_before(t, FROM, TO, Some(owner))?;
                let light = read_contacted(t, FROM, TO, Some(owner), true)?;
                assert_eq!(call_ids(&light), call_ids(&legacy), "{owner}");
                let list = |rows: &[ReviewRow]| {
                    rows.iter()
                        .filter(|r| r.status != ST_QUEUED)
                        .map(|r| call_row(r, &ids))
                        .collect::<Vec<_>>()
                };
                assert_eq!(list(&light), list(&legacy), "{owner}");
            }
            let mine = read_contacted(t, FROM, TO, Some("o1"), true)?;
            assert_eq!(call_ids(&mine), vec!["r11", "r4", "r9", "r2", "r1"]);
            let rows: Vec<TalkCallRow> = mine
                .iter()
                .filter(|r| r.status != ST_QUEUED)
                .map(|r| call_row(r, &ids))
                .collect();
            assert_eq!(rows.len(), 4, "枠待ちの r11 は一覧に出さない");
            let r1 = rows.last().unwrap();
            assert_eq!(
                (
                    r1.hubspot_call_id.as_str(),
                    r1.status.as_str(),
                    r1.outcome.as_deref()
                ),
                ("r1", "judged", Some("決まらなかった"))
            );
            assert_eq!(r1.summary.as_deref(), Some("r1 のまとめ"));
            let early_mark = r1.items.iter().find(|i| i.id == "early_date").unwrap();
            assert_eq!(early_mark.status, "not_done");
            // 判定前の行は項目も要約も出さない
            let r4 = &rows[0];
            assert_eq!(
                (r4.hubspot_call_id.as_str(), r4.status.as_str()),
                ("r4", "waiting")
            );
            assert!(r4.items.is_empty() && r4.summary.is_none());
            assert_eq!(
                (rows[1].hubspot_call_id.as_str(), rows[1].status.as_str()),
                ("r9", "failed")
            );
            // 行が 0 件の期間は、印を読まずに空で返す
            assert!(read_contacted(t, TO, "2026-10-18T00:00:00Z", None, false)?.is_empty());
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reads_wait_for_a_free_slot_and_give_it_back_when_done() {
        let (audit, conn, _) = start_sqlite_audit_with_queries().await;
        seed_calls(&conn);
        let st = Arc::new(TalkState::default());
        assert_eq!(READ_PERMITS, 2);
        assert_eq!(st.reads.0.available_permits(), READ_PERMITS);
        // 枠を全部使っておく
        let mut held: Vec<_> = (0..READ_PERMITS)
            .map(|_| st.reads.0.clone().try_acquire_owned().unwrap())
            .collect();
        let task = {
            let (st, audit) = (st.clone(), audit.clone());
            tokio::spawn(async move {
                st.load_contacted(
                    &audit,
                    (FROM.to_string(), TO.to_string()),
                    Some("o2".into()),
                    true,
                )
                .await
            })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!task.is_finished(), "枠が空くまで Turso を読まない");
        // 1 つ空くと進み、読み終えたら枠が戻る
        drop(held.pop());
        let rows = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("枠が空いても読み込みが進まない")
            .unwrap()
            .unwrap();
        assert_eq!(call_ids(&rows), vec!["r7", "r6", "r5"]);
        assert_eq!(st.reads.0.available_permits(), 1);
        drop(held);
        assert_eq!(st.reads.0.available_permits(), READ_PERMITS);
    }
}
