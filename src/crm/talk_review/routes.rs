//! `/api/crm/talk-review/*` (Google ログインした社内の人なら誰でも。役割は問わない。パスワードログインは拒否)。
//! 認可で落ちる限り HubSpot・Zoom・Turso は呼ばない。

use std::collections::{BTreeSet, HashMap};
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
use crate::hubspot::deep_link::hubspot_portal_id;
use crate::AppState;

/// アポ数の内訳の待ちの上限 (62 日の全体で HubSpot を約 110 回読む。画面の待ちは 30 秒)
const APO_BREAKDOWN_DEADLINE: Duration = Duration::from_secs(28);
/// 期間の最大日数
pub const MAX_PERIOD_DAYS: i64 = 62;
const ROWS_TTL: Duration = Duration::from_secs(60);
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
                .map(|id| TalkItemMark {
                    id: id.clone(),
                    status: r
                        .items
                        .iter()
                        .find(|i| &i.id == id)
                        .map_or("na", |i| i.status.as_str())
                        .to_string(),
                })
                .collect()
        } else {
            vec![]
        },
        summary: r.summary.clone().filter(|_| judged),
    }
}

// ---------------------------------------------------------------------------
// プロセス内の状態
// ---------------------------------------------------------------------------

type RowsCache = Mutex<HashMap<(String, String), (Instant, Arc<Vec<ReviewRow>>)>>;
type TranscriptCache = Mutex<HashMap<String, (Instant, Result<Vec<TalkTurn>, ZoomError>)>>;

/// 期間の行のうち、接触した電話 (接触の判定で `contacted = 1`) だけ。集計も一覧もこの行で数える。
/// 接触の印が読めなければ全体を失敗にする (定義の違う数を混ぜて出さない)
async fn contacted_in(
    audit: &crate::audit::AuditDb,
    from: String,
    to: String,
    owner: Option<String>,
) -> Result<Vec<ReviewRow>, String> {
    // 印の時刻は Zoom の通話ログ、行の時刻は HubSpot の Call。境目の電話を取りこぼさないよう少し広げて読む
    let pad = chrono::Duration::milliseconds(super::zoom::CALLER_WINDOW_MS);
    let widen = |s: &str, d: chrono::Duration| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|t| store::ts_text(t.with_timezone(&chrono::Utc) + d))
            .unwrap_or_else(|_| s.to_string())
    };
    let (mfrom, mto) = (widen(&from, -pad), widen(&to, pad));
    store::blocking(audit, move |t| {
        let rows = store::rows_in(t, &from, &to, owner.as_deref())?;
        if rows.is_empty() {
            return Ok(rows);
        }
        let marks = contact_store::contacted_marks(t, &mfrom, &mto)?;
        Ok(super::target::contacted_rows(rows, &marks))
    })
    .await
}

#[derive(Default)]
pub struct TalkState {
    pub(super) counts: CountsCache,
    apo_breakdown: Arc<super::apo_breakdown::BreakdownCache>,
    rows: RowsCache,
    transcripts: TranscriptCache,
    zoom: OnceLock<Option<Arc<ZoomClient>>>,
}

pub fn shared() -> Arc<TalkState> {
    static S: OnceLock<Arc<TalkState>> = OnceLock::new();
    S.get_or_init(|| Arc::new(TalkState::default())).clone()
}

impl TalkState {
    async fn rows_in(
        &self,
        state: &AppState,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Arc<Vec<ReviewRow>>, Response> {
        let (a, b) = utc_bounds(from, to);
        let key = (a.clone(), b.clone());
        if let Some((at, v)) = self.rows.lock().unwrap().get(&key) {
            if at.elapsed() < ROWS_TTL {
                return Ok(v.clone());
            }
        }
        let Some(audit) = state.audit.clone() else {
            return Err(error_json(
                StatusCode::SERVICE_UNAVAILABLE,
                "not_configured",
            ));
        };
        let rows = contacted_in(&audit, a, b, None)
            .await
            .map_err(|_| error_json(StatusCode::SERVICE_UNAVAILABLE, "review_unavailable"))?;
        let rows = Arc::new(rows);
        let mut g = self.rows.lock().unwrap();
        if g.len() > 20 {
            g.clear();
        }
        g.insert(key, (Instant::now(), rows.clone()));
        Ok(rows)
    }

    pub(super) fn zoom_client(&self) -> Option<Arc<ZoomClient>> {
        self.zoom.get_or_init(ZoomClient::from_env).clone()
    }

    async fn transcript(&self, zoom_id: &str) -> Result<Vec<TalkTurn>, ZoomError> {
        if let Some((at, v)) = self.transcripts.lock().unwrap().get(zoom_id) {
            if at.elapsed() < TRANSCRIPT_TTL {
                return v.clone();
            }
        }
        let zoom = self
            .zoom
            .get_or_init(ZoomClient::from_env)
            .clone()
            .ok_or(ZoomError::NotConfigured)?;
        let r = zoom.fetch(zoom_id).await;
        if cacheable(&r) {
            let mut g = self.transcripts.lock().unwrap();
            if g.len() >= TRANSCRIPT_MAX {
                g.clear();
            }
            g.insert(zoom_id.to_string(), (Instant::now(), r.clone()));
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
    let snap = st.counts.snapshot(from, to);
    if !snap.counting.is_empty() {
        let (st, client, team) = (st.clone(), client.clone(), team.clone());
        tokio::spawn(async move {
            if let Err(e) = st.counts.fill(&client, &team, from, to, now, false).await {
                tracing::warn!(error = %e.error_kind(), "talk review: counting failed");
            }
        });
    }
    let deals = snap.deals;
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
        &snap.counting,
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
    let (a, b) = utc_bounds(from, to);
    let o2 = owner.clone();
    let rows = match contacted_in(&audit, a, b, Some(o2)).await {
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
    let (transcript, transcript_error) = match row.zoom_call_log_id.as_deref() {
        None => (None, Some("録音が残っていません".to_string())),
        Some(z) => match tokio::time::timeout(CRM_REQUEST_DEADLINE, shared().transcript(z)).await {
            Ok(Ok(t)) => (Some(t), None),
            Ok(Err(e)) => (None, Some(transcript_error_text(&e).to_string())),
            Err(_) => (
                None,
                Some(transcript_error_text(&ZoomError::Transient(String::new())).to_string()),
            ),
        },
    };
    let ids = check_ids();
    let judged = row.status == ST_JUDGED;
    let items = loaded()
        .checks
        .checks
        .iter()
        .map(
            |c| match row.items.iter().find(|i| i.id == c.id).filter(|_| judged) {
                Some(i) => TalkItemDetail {
                    id: c.id.clone(),
                    status: i.status.clone(),
                    evidence: i.evidence.clone(),
                    quote: i.quote.clone(),
                    advice: i.advice.clone(),
                    note: i.note.clone(),
                },
                None => TalkItemDetail {
                    id: c.id.clone(),
                    status: "na".into(),
                    evidence: vec![],
                    quote: None,
                    advice: None,
                    note: None,
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
            }],
            summary: Some("要約".into()),
            note: None,
        };
        let ids = check_ids();
        let c = call_row(&r, &ids);
        assert_eq!(c.status, "judged");
        assert_eq!(c.items.len(), 9);
        assert_eq!(
            (c.items[2].id.as_str(), c.items[2].status.as_str()),
            ("early_date", "not_done")
        );
        assert_eq!(c.items[0].status, "na");
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
        assert_eq!(v["checks"].as_array().unwrap().len(), 9);
        assert_eq!(v["rules"]["item_min_applicable"], json!(5));
        assert_eq!(v["rules"]["appointment_min_dials"], json!(100));
        assert_eq!(v["total"]["items"].as_array().unwrap().len(), 9);
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
}
