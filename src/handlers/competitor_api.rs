//! 競合調査の JSON API (`GET /api/competitor/options`、`POST /api/competitor/report`)。
//!
//! 旧画面 (`/report/competitor`、HTML を返す) と同じ入力・同じ集計を使い、結果を `CompetitorReport` (JSON) で返す。
//! 旧画面の挙動は変えない (共通部分は親モジュール `competitor.rs` にある)。
//!
//! 追加で持つ状態は 2 つ。どちらもこのプロセスのメモリだけ (再デプロイで消える)。
//! - 生成したレポートを 30 分保持する (`ReportStore`)。`report_id` は 32 バイトの乱数 (16 進 64 桁) で、
//!   作った本人のメールでしか読めない。PR-3 の PDF 作成が同じレポートを再利用する (Google を再取得しない)。
//! - 同じユーザーの同時送信を 1 本に制限する (`InFlight`)。2 本目は 429 `report_in_progress`。
//!   ガード (`InFlightGuard`) は Drop で解放するので、失敗・panic・接続切断でも残らない。
//!
//! エラーは `CompetitorError { error: コード, message: 固定の日本語 }`。CSV 解析エラーなど内部ライブラリや
//! 外部入力由来の文字列は画面に出さず、詳細は `tracing::warn!` (Render Logs) にだけ出す。
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::Multipart;
use rand::RngCore;
use serde::Serialize;
use tower_sessions::Session;
use ts_rs::TS;

use super::{
    analyze, collect_context, parse_request, read_form, AnalyzeError, FormError, ReportRequest,
};
use crate::handlers::survey::report_html::competitor_model::{
    build_competitor_report, CompetitorReport,
};
use crate::indeed::data::snapshot;
use crate::AppState;

/// CSV の行数の上限 (ユーザー決定 2026-10-05)。レコード数で数える (ヘッダー行は含まない)。
pub const MAX_CSV_ROWS: usize = 50_000;
/// 生成したレポートの保持時間 (ユーザー決定 2026-10-05)。
pub const REPORT_TTL_SECS: u64 = 30 * 60;
/// 保持するレポート数の上限 (メモリの上限)。超えたら古い順に捨てる。
const STORE_CAP: usize = 200;

// ---------------------------------------------------------------- エラー

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CompetitorErrorCode {
    CsvMissing,
    CsvUnreadable,
    CsvTooLarge,
    CsvTooManyRows,
    CsvParseFailed,
    NoIndeedJobs,
    FieldTooLong,
    InvalidSourceType,
    InvalidWageMode,
    InvalidPrefecture,
    /// 同じユーザーの前のレポート作成がまだ終わっていない。
    ReportInProgress,
    /// 保持期限切れ・存在しない・他人の `report_id` (区別しない)。
    ReportNotFound,
}

/// エラー応答の本文。`message` は利用者に見せてよい固定の日本語。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct CompetitorError {
    pub error: CompetitorErrorCode,
    pub message: String,
}

pub struct ApiError {
    pub status: StatusCode,
    pub body: CompetitorError,
}

impl ApiError {
    fn new(status: StatusCode, error: CompetitorErrorCode, message: &str) -> Self {
        Self {
            status,
            body: CompetitorError {
                error,
                message: message.to_owned(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut res = (self.status, Json(self.body)).into_response();
        res.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        res
    }
}

fn form_error(e: FormError) -> ApiError {
    use CompetitorErrorCode as Code;
    // 旧画面の文言をそのまま使えるもの以外は API 用の固定文にする。
    let message = match e.code {
        Code::CsvTooLarge => "CSVのサイズが上限(20MB)を超えています。",
        Code::CsvUnreadable => {
            "CSVを読み込めませんでした。ファイルサイズと形式を確認してください。"
        }
        _ => e.message,
    };
    ApiError::new(e.status, e.code, message)
}

pub(super) fn analysis_error(e: AnalyzeError) -> ApiError {
    use CompetitorErrorCode as Code;
    match e {
        AnalyzeError::Parse(detail) => {
            // 詳細 (ライブラリのエラー文) はログだけ。画面には出さない。
            tracing::warn!(detail = %detail, "competitor CSV parse failed");
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                Code::CsvParseFailed,
                "CSVを解析できませんでした。列名と形式を確認してください。",
            )
        }
        AnalyzeError::TooManyRows(rows) => {
            tracing::info!(rows, "competitor CSV over row limit");
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                Code::CsvTooManyRows,
                &format!(
                    "CSVの行数が上限({}行)を超えています。分割して作成してください。",
                    MAX_CSV_ROWS
                ),
            )
        }
        AnalyzeError::NoIndeed => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            Code::NoIndeedJobs,
            "分析できるIndeed求人がありません。CSVの列と内容を確認してください。",
        ),
    }
}

// ---------------------------------------------------------------- 30 分保持

struct Stored {
    owner: String,
    at: Instant,
    report: Arc<CompetitorReport>,
}

/// 生成したレポートの一時保持。時刻は呼び出し側から渡す (テストで 30 分後を作るため)。
pub struct ReportStore {
    ttl: Duration,
    cap: usize,
    inner: Mutex<HashMap<String, Stored>>,
}

fn normalize_user(user: &str) -> String {
    user.trim().to_lowercase()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // 他スレッドが panic しても保持は使い続ける (データは壊れていない)。
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ReportStore {
    pub fn new(ttl: Duration, cap: usize) -> Self {
        Self {
            ttl,
            cap,
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn purge(&self, map: &mut HashMap<String, Stored>, now: Instant) {
        let ttl = self.ttl;
        map.retain(|_, s| now.saturating_duration_since(s.at) <= ttl);
    }

    /// 保存して `report_id` (乱数 32 バイトの 16 進 64 桁) を返す。
    pub fn insert(&self, owner: &str, report: CompetitorReport, now: Instant) -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let mut map = lock(&self.inner);
        self.purge(&mut map, now);
        while map.len() >= self.cap {
            let Some(oldest) = map.iter().min_by_key(|(_, s)| s.at).map(|(k, _)| k.clone()) else {
                break;
            };
            map.remove(&oldest);
        }
        map.insert(
            id.clone(),
            Stored {
                owner: normalize_user(owner),
                at: now,
                report: Arc::new(report),
            },
        );
        id
    }

    /// 本人が期限内に読む。期限切れ・存在しない・他人の ID はどれも `None` (区別しない)。
    /// PR-3 の PDF 作成が使う。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn get(&self, owner: &str, id: &str, now: Instant) -> Option<Arc<CompetitorReport>> {
        let mut map = lock(&self.inner);
        self.purge(&mut map, now);
        let stored = map.get(id)?;
        (stored.owner == normalize_user(owner)).then(|| stored.report.clone())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        lock(&self.inner).len()
    }
}

pub(super) fn store() -> &'static ReportStore {
    static STORE: OnceLock<ReportStore> = OnceLock::new();
    STORE.get_or_init(|| ReportStore::new(Duration::from_secs(REPORT_TTL_SECS), STORE_CAP))
}

// ---------------------------------------------------------------- 同時送信

/// 作成中のユーザー一覧。同じユーザーは 1 本だけ。
pub struct InFlight {
    users: Mutex<HashSet<String>>,
}

pub struct InFlightGuard<'a> {
    owner: &'a InFlight,
    key: String,
}

impl InFlight {
    pub fn new() -> Self {
        Self {
            users: Mutex::new(HashSet::new()),
        }
    }

    /// 取れたらガードを返す (Drop で解放)。既に作成中なら `None`。
    pub fn try_acquire(&self, user: &str) -> Option<InFlightGuard<'_>> {
        let key = normalize_user(user);
        // ロックは先に外す。ガードを作る前に Drop が走ると、ロックの二重取得 (デッドロック) と、
        // 他の人が持っている枠の誤解放になる。
        let inserted = lock(&self.users).insert(key.clone());
        inserted.then(|| InFlightGuard { owner: self, key })
    }
}

impl Default for InFlight {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        lock(&self.owner.users).remove(&self.key);
    }
}

pub(super) fn in_flight() -> &'static InFlight {
    static IN_FLIGHT: OnceLock<InFlight> = OnceLock::new();
    IN_FLIGHT.get_or_init(InFlight::new)
}

// ---------------------------------------------------------------- ハンドラ

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct CompetitorOptions {
    /// Indeed 採用市場の職種 (五十音ではなく文字コード順)。
    pub titles: Vec<String>,
    pub prefectures: Vec<String>,
    /// false のとき職種の選択肢は空 (CSV の競合調査だけ使える)。
    pub market_available: bool,
}

pub async fn api_options(State(state): State<Arc<AppState>>) -> Json<CompetitorOptions> {
    let market = tokio::task::spawn_blocking(move || {
        state.indeed_db.as_ref().and_then(|db| snapshot(db).ok())
    })
    .await
    .ok()
    .flatten();
    let market_available = market.is_some();
    let titles = market
        .map(|s| {
            let mut titles: Vec<String> = s.titles.iter().map(|t| t.name.clone()).collect();
            titles.sort_unstable();
            titles
        })
        .unwrap_or_default();
    Json(CompetitorOptions {
        titles,
        prefectures: crate::models::job_seeker::PREFECTURE_ORDER
            .iter()
            .map(|p| (*p).to_owned())
            .collect(),
        market_available,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct CompetitorReportResponse {
    /// この ID で 30 分間 (`expires_in_secs`) だけ同じレポートを再利用できる。作った本人だけ。
    pub report_id: String,
    pub expires_in_secs: u32,
    pub report: CompetitorReport,
}

pub async fn api_report(
    State(state): State<Arc<AppState>>,
    session: Session,
    mut multipart: Multipart,
) -> Response {
    let user: Option<String> = session
        .get(crate::auth::SESSION_USER_KEY)
        .await
        .ok()
        .flatten();
    let Some(user) = user else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, "application/json")],
            crate::auth::AUTH_REQUIRED_JSON,
        )
            .into_response();
    };
    // 本文を読む前に取る (アップロード中の 2 本目も止める)。関数を抜けるとき (失敗・切断でも) 解放される。
    let Some(_slot) = in_flight().try_acquire(&user) else {
        return ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            CompetitorErrorCode::ReportInProgress,
            "前のレポートを作成中です。完了してからもう一度お試しください。",
        )
        .into_response();
    };
    match build(&state, &user, &mut multipart).await {
        Ok(body) => {
            let mut res = Json(body).into_response();
            res.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            res
        }
        Err(e) => e.into_response(),
    }
}

async fn build(
    state: &Arc<AppState>,
    user: &str,
    multipart: &mut Multipart,
) -> Result<CompetitorReportResponse, ApiError> {
    let mut req: ReportRequest = read_form(multipart)
        .await
        .and_then(parse_request)
        .map_err(form_error)?;
    let csv = std::mem::take(&mut req.csv);
    let agg = analyze(csv, req.source, req.mode, &req.pref, Some(MAX_CSV_ROWS))
        .await
        .map_err(analysis_error)?;
    let (indeed, google, population) = collect_context(state.clone(), &req).await;
    let mut report = build_competitor_report(
        &agg,
        req.top_n,
        &req.survey_title,
        &indeed,
        &google,
        &population,
    );
    if !req.top_n_raw.is_empty() {
        report.meta.top_n_requested = Some(req.top_n_raw.clone());
        if req.top_n_raw.parse::<i64>().ok() != Some(req.top_n as i64) {
            report.meta.warnings.push(format!(
                "表示件数の指定を {} 件に調整しました (指定できるのは 1〜200 の整数です)。",
                req.top_n
            ));
        }
    }
    let report_id = store().insert(user, report.clone(), Instant::now());
    Ok(CompetitorReportResponse {
        report_id,
        expires_in_secs: REPORT_TTL_SECS as u32,
        report,
    })
}
