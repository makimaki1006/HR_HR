//! Scout（スカウト自動化）中央バックエンド
//!
//! OpenWorkScoutRPA のローカルアプリ（顧客PC）が HTTP 経由で叩く API を `/scout/*` に提供する。
//! - データは専用 Turso DB（`SCOUT_TURSO_URL`/`SCOUT_TURSO_TOKEN`）に保存。HR_HR 本体DBには触れない。
//! - 認証は HR_HR の cookie/ドメイン認証とは独立した「トークン認証」。`/scout/*` は
//!   HR_HR の require_auth/CSRF をバイパスし、各エンドポイントで自前トークンを検証する。
//! - パスワードは bcrypt（HR_HR 既存依存を再利用）。
//! - config は workspace ごとの JSON ドキュメントとして kv_settings に保存（ローカルアプリはJSONで受領）。
//!
//! 注意: `TursoDb` は reqwest::blocking を使うため、DB呼び出しは必ず `tokio::task::spawn_blocking`
//! 内で実行する（async コンテキストで直接呼ぶと runtime drop panic になる）。各ハンドラは
//! 同期の `*_core` 関数を spawn_blocking で包む構造にしている。

use std::collections::HashMap;
use std::sync::Arc;

use std::sync::OnceLock;

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Aes256Gcm,
};
use axum::{
    extract::{Path, Query, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderMap, HeaderValue, StatusCode,
    },
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::config::SCOUT_CREDENTIALS_KEY_ENV;
use crate::db::turso_http::{ToSqlTurso, TursoDb};
use crate::AppState;

const SESSION_TTL_DAYS: i64 = 7;
const CONFIG_KEY: &str = "__config__";
const STATE_KEY: &str = "__state__";
const CREDENTIAL_KEY_PREFIX: &str = "__credentials__:";
const CREDENTIAL_UPSERT_SQL: &str =
    "INSERT INTO kv_settings(workspace_id,key,value) VALUES(?,?,?) \
     ON CONFLICT(workspace_id,key) DO UPDATE SET value=excluded.value";
const CONFIG_PATCH_SQL: &str = "INSERT INTO kv_settings(workspace_id,key,value) VALUES(?,?,?) \
     ON CONFLICT(workspace_id,key) DO UPDATE SET \
     value=json_patch(COALESCE(kv_settings.value,'{}'), ?)";

// ===== 実行ログ(run_logs)の SQL / 定数 =====
//
// send_history は「送れた分」だけが載る表で、弾かれた候補者・落ちた理由は中央に
// 1行も残っていなかった(run_logs が 0 行)。run_logs はその「結末の全部」を受ける。

/// 実行ログ1行の INSERT。**ログなので冪等化しない**。
/// 同じ候補者が何度も弾かれたなら、その回数ぶん行が残るのが正しい
/// (send_history の冪等ガードと役割が違う)。
const RUN_LOG_INSERT_SQL: &str =
    "INSERT INTO run_logs(ts,campaign_id,iter,candidate_web_id,subject_chars,body_chars,\
     api_seconds,status,workspace_id,user_id,platform,error_kind,error_step,error_detail,error_url) \
     VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";

/// `campaigns` 表があるかの判定。
///
/// キャンペーン名は現状 `kv_settings.__config__` の JSON 側にあり、中央の
/// scout DB に `campaigns` 表があるかは**こちらから確認できない**(Turso へは触らない)。
/// 無い DB で LEFT JOIN すると SELECT 全体が落ちて CSV が1行も出せなくなるので、
/// 先に存在を見て、無ければ campaign_name を空にした SQL に切り替える。
const RUN_LOG_CAMPAIGNS_EXISTS_SQL: &str =
    "SELECT name AS name FROM sqlite_master WHERE type='table' AND name='campaigns'";

/// CSV エクスポートの本体 SQL(`campaigns` 表がある場合)。
///
/// `from`/`to`/`platform` は**空文字で「指定なし」**を表し、SQL 側で `?=''` を見る。
/// 文字列連結で WHERE を組み立てないのは、SQL を1本の定数として
/// ローカル SQLite にそのまま流して検証できる形にしておくため。
/// `LIMIT 50001` は上限(50,000)+1 件取って、打ち切りが起きたことを呼び出し元に知らせるため。
const RUN_LOG_EXPORT_SQL: &str = "SELECT l.ts AS ts, l.campaign_id AS campaign_id, \
     COALESCE(c.name,'') AS campaign_name, l.platform AS platform, \
     l.candidate_web_id AS candidate_web_id, l.status AS status, \
     l.subject_chars AS subject_chars, l.body_chars AS body_chars, \
     l.api_seconds AS api_seconds, l.error_kind AS error_kind, l.error_step AS error_step, \
     l.error_detail AS error_detail, l.error_url AS error_url, l.user_id AS user_id \
     FROM run_logs l \
     LEFT JOIN campaigns c ON c.id=l.campaign_id AND c.workspace_id=l.workspace_id \
     WHERE l.workspace_id=? \
       AND (?='' OR substr(l.ts,1,10)>=?) \
       AND (?='' OR substr(l.ts,1,10)<=?) \
       AND (?='' OR l.platform=?) \
     ORDER BY l.ts LIMIT 50001";

/// `campaigns` 表が無い DB 用。列の並び・名前は `RUN_LOG_EXPORT_SQL` と同一に保つ
/// (CSV 生成側が列名で引くので、ここがズレると列が空になる)。
const RUN_LOG_EXPORT_SQL_WITHOUT_CAMPAIGNS: &str = "SELECT l.ts AS ts, \
     l.campaign_id AS campaign_id, '' AS campaign_name, l.platform AS platform, \
     l.candidate_web_id AS candidate_web_id, l.status AS status, \
     l.subject_chars AS subject_chars, l.body_chars AS body_chars, \
     l.api_seconds AS api_seconds, l.error_kind AS error_kind, l.error_step AS error_step, \
     l.error_detail AS error_detail, l.error_url AS error_url, l.user_id AS user_id \
     FROM run_logs l \
     WHERE l.workspace_id=? \
       AND (?='' OR substr(l.ts,1,10)>=?) \
       AND (?='' OR substr(l.ts,1,10)<=?) \
       AND (?='' OR l.platform=?) \
     ORDER BY l.ts LIMIT 50001";

/// CSV の見出し行。列の数・順序は `run_log_csv_row` と1対1で対応させる。
const RUN_LOG_CSV_HEADER: &str = "ts,campaign_id,campaign_name,platform,candidate_web_id,status,\
subject_chars,body_chars,api_seconds,error_kind,error_step,error_detail,error_url,user_id";

/// CSV の列数(打ち切り行のカンマ数と見出しの整合をテストで縛るため)。
const RUN_LOG_CSV_COLUMNS: usize = 14;

/// CSV に出す最大行数。これを超えた分は**黙って捨てない**
/// (`X-Truncated` ヘッダと最終行の注記で呼び出し元に見せる)。
const RUN_LOG_EXPORT_MAX_ROWS: usize = 50_000;

/// 打ち切ったことを CSV 本文にも残す注記(ヘッダを見ない人が Excel で開く前提)。
const RUN_LOG_TRUNCATED_NOTICE: &str =
    "上限50000行で打ち切りました。from/to で期間を狭めて取り直してください";

/// `error_detail` の保存上限(文字数)。暴走したページの文面で表が膨らむのを止める。
const RUN_LOG_ERROR_DETAIL_MAX_CHARS: usize = 500;

/// `error_url` の保存上限(文字数)。クエリ付きの長大 URL を切る。
const RUN_LOG_ERROR_URL_MAX_CHARS: usize = 300;

/// `run_logs.platform` の後付け。定数に出してあるのは、テストが
/// **実装と同じ文**をローカル SQLite に流して検証できるようにするため。
const RUN_LOG_PLATFORM_ALTER_SQL: &str = "ALTER TABLE run_logs ADD COLUMN platform TEXT";

/// `run_logs` の失敗内容4列の後付け。SQLite の ALTER TABLE は1文1列なので4本に分ける。
const RUN_LOG_ERROR_COLUMN_ALTER_SQL: [&str; 4] = [
    "ALTER TABLE run_logs ADD COLUMN error_kind TEXT",
    "ALTER TABLE run_logs ADD COLUMN error_step TEXT",
    "ALTER TABLE run_logs ADD COLUMN error_detail TEXT",
    "ALTER TABLE run_logs ADD COLUMN error_url TEXT",
];

/// 新規 workspace の既定 config（空キャンペーン＋既定設定）。ローカルアプリが即使える状態にする。
const DEFAULT_CONFIG_JSON: &str = r#"{
  "$schema_version": 1,
  "campaigns": [],
  "schedule": {"active_hours": {"start": "09:00", "end": "18:00"}, "weekdays_only": true, "outside_hours_action": "sleep", "min_gap_between_sends_sec": 30},
  "runtime": {"gemini_model": "gemini-3.1-flash-lite", "auto_resend": "送信する", "dry_run": false, "max_iterations_per_session": 0},
  "limits": {"daily_total": 0, "daily_per_platform": {}, "daily_per_campaign": 0},
  "system": {"prompt_template_path": "./gemini_prompt_template.txt", "profile_dir": "./.chrome_profile_campaign", "state_file": "./campaign_state.json", "log_csv": "./campaign_log.csv", "verify_log": "./campaign_verify.txt", "chrome_path": "auto"},
  "gemini_api_key_env": "GEMINI_API_KEY"
}"#;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/scout/api/health", get(health))
        .route("/scout/api/auth/login", post(login))
        .route("/scout/api/auth/me", get(me))
        .route("/scout/api/auth/logout", post(logout))
        .route(
            "/scout/api/config",
            get(get_config).post(save_config).patch(patch_config),
        )
        .route("/scout/api/credentials/status", get(get_credentials_status))
        .route(
            "/scout/api/credentials/{platform}",
            get(get_platform_credentials),
        )
        .route("/scout/api/credentials", post(save_platform_credentials))
        .route("/scout/api/state", get(get_state).post(save_state))
        .route("/scout/api/sent", post(sent))
        .route("/scout/api/has-sent", get(has_sent))
        .route("/scout/api/stats", get(stats))
        // 送信できなかった候補者の結末と理由も中央に残す(記録と取り出し)。
        .route("/scout/api/run-log", post(run_log))
        .route("/scout/api/run-log/export", get(run_log_export))
        .route("/scout/api/killswitch", get(killswitch))
        .route("/scout/api/admin/killswitch", post(admin_killswitch))
        .route("/scout/api/admin/disable", post(admin_disable))
        .route("/scout/api/admin/provision", post(provision))
        .route(
            "/scout/api/admin/users",
            get(admin_list_users).post(admin_create_user),
        )
        .route(
            "/scout/api/admin/reset-password",
            post(admin_reset_password),
        )
        // 1社複数ユーザー: 自分の workspace の担当者を管理する(master のセッションが要る)。
        .route(
            "/scout/api/admin/members",
            get(admin_list_members).post(admin_add_member),
        )
        .route("/scout/api/admin/members/remove", post(admin_remove_member))
        .route(
            "/scout/api/admin/members/disabled",
            post(admin_set_member_disabled),
        )
}

// ===== 型・共通ヘルパー =====

/// コア関数(同期)のエラー: (HTTPステータス, メッセージ)
type CoreErr = (StatusCode, String);
type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;
type CredentialApiResult = Result<(HeaderMap, Json<Value>), (StatusCode, HeaderMap, Json<Value>)>;
/// CSV を返すエンドポイント用。成功は (ヘッダ, CSV本文)、失敗は既存と同じ JSON エラー。
type CsvApiResult = Result<(HeaderMap, String), (StatusCode, Json<Value>)>;

fn cerr(code: StatusCode, msg: impl Into<String>) -> CoreErr {
    (code, msg.into())
}

/// scout_db を clone で取得(blocking用に move する)。未設定なら 503。
fn take_db(state: &AppState) -> Result<TursoDb, CoreErr> {
    state
        .scout_db
        .clone()
        .ok_or_else(|| cerr(StatusCode::SERVICE_UNAVAILABLE, "scout DB が未設定です"))
}

/// コア(同期)を spawn_blocking で実行し、HTTPレスポンスへ変換する。
async fn run<F>(f: F) -> ApiResult
where
    F: FnOnce() -> Result<Value, CoreErr> + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Ok(Json(v)),
        Ok(Err((code, msg))) => Err((code, Json(json!({ "error": msg })))),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal task error" })),
        )),
    }
}

fn credential_response_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store, private"));
    headers
}

async fn run_credentials<F>(f: F) -> CredentialApiResult
where
    F: FnOnce() -> Result<Value, CoreErr> + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(value)) => Ok((credential_response_headers(), Json(value))),
        Ok(Err((code, message))) => Err((
            code,
            credential_response_headers(),
            Json(json!({ "error": message })),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            credential_response_headers(),
            Json(json!({ "error": "internal task error" })),
        )),
    }
}

fn now_str() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn get_str(row: &HashMap<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// リクエストボディから文字列を1つ取る(欠落・型違い・空白のみは空文字)。
fn body_str(body: &Value, key: &str) -> String {
    body.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// 先頭 `max` 文字で切る。**バイトでなく文字**で数える(日本語の文面が入るため)。
fn clip_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn token_from(headers: &HeaderMap) -> String {
    headers
        .get("x-auth-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn new_token() -> String {
    format!("{}{}", new_id(), new_id())
}

struct SessionUser {
    user_id: String,
    email: String,
    name: String,
    workspace_id: String,
    role: String,
}

/// トークンからログイン中ユーザーを解決（期限切れは None）。※同期。呼び出しは spawn_blocking 内で。
fn current_user(db: &TursoDb, token: &str) -> Option<SessionUser> {
    if token.is_empty() {
        return None;
    }
    // role 列を後付けする（一度だけ ALTER）。JOIN で u.role を引く前に必須。
    ensure_user_role_column(db);
    ensure_workspace_members_table(db);
    let t = token.to_string();
    let params: [&dyn ToSqlTurso; 1] = [&t];
    // role は**そのセッションが入っている workspace での役割**を解決する
    // （session 発行後の変更も即反映）。users.role だけを見ると、role 列を
    // 後付けした時点で既に居た所有者が既定値の 'member' になり、その会社の
    // 管理者が担当者管理をできなくなる。所有者は master として扱う。
    let rows = db
        .query(
            "SELECT s.user_id,s.email,s.name,s.workspace_id,s.expires_at,\
             COALESCE(m.role, \
                      CASE WHEN w.owner_user_id=s.user_id THEN 'master' END, \
                      u.role, 'member') AS role \
             FROM auth_sessions s \
             LEFT JOIN users u ON u.id=s.user_id \
             LEFT JOIN workspaces w ON w.id=s.workspace_id \
             LEFT JOIN workspace_members m \
                    ON m.workspace_id=s.workspace_id AND m.user_id=s.user_id \
             WHERE s.token=?",
            &params,
        )
        .ok()?;
    let r = rows.first()?;
    if get_str(r, "expires_at").as_str() <= now_str().as_str() {
        return None;
    }
    Some(SessionUser {
        user_id: get_str(r, "user_id"),
        email: get_str(r, "email"),
        name: get_str(r, "name"),
        workspace_id: get_str(r, "workspace_id"),
        role: get_str(r, "role"),
    })
}

/// master ロールを要求（ユーザー管理API用）。member/未ログインは 403/401。
fn require_master(db: &TursoDb, token: &str) -> Result<SessionUser, CoreErr> {
    let u = require_user(db, token)?;
    if u.role != "master" {
        return Err(cerr(StatusCode::FORBIDDEN, "管理者(master)権限が必要です"));
    }
    Ok(u)
}

fn require_user(db: &TursoDb, token: &str) -> Result<SessionUser, CoreErr> {
    require_credentials_token(token)?;
    current_user(db, token)
        .ok_or_else(|| cerr(StatusCode::UNAUTHORIZED, "未ログイン(トークンが無効です)"))
}

fn require_credentials_user(db: &TursoDb, token: &str) -> Result<SessionUser, CoreErr> {
    let user = require_user(db, token)?;
    if user.workspace_id.trim().is_empty() {
        return Err(cerr(StatusCode::UNAUTHORIZED, "workspace is required"));
    }
    Ok(user)
}

/// users テーブルへ `disabled` 列を後付けする(解約遮断用)。プロセス生存中に一度だけ実行。
/// 既に列がある/ALTER 非対応でも失敗を無視する(冪等)。※同期。spawn_blocking 内で呼ぶこと。
fn ensure_user_disabled_column(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        // 既に列が存在すると Turso はエラーを返すが、それは正常系として無視する。
        let _ = db.execute(
            "ALTER TABLE users ADD COLUMN disabled INTEGER DEFAULT 0",
            &[],
        );
    });
}

/// users テーブルへ `role` 列を後付けする(master/member の権限分離用)。プロセス生存中に一度だけ。
/// 既定は 'member'。既存ユーザーは全員 member 扱いになる(master は明示昇格が必要)。※同期。
fn ensure_user_role_column(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        let _ = db.execute(
            "ALTER TABLE users ADD COLUMN role TEXT DEFAULT 'member'",
            &[],
        );
    });
}

/// `workspace_members` テーブルを用意する(1社に複数の担当者を置くため)。プロセス生存中に一度だけ。
///
/// これが無いと、user と workspace の紐付けが `workspaces.owner_user_id` の1本しかなく、
/// 所有者は1人しか書けないため**1 workspace に2人目を置けない**。
/// 実運用では複数PCで新卒媒体と中途媒体を分けて同時に回しており、担当者ごとの
/// アカウントが要る。※同期。spawn_blocking 内で呼ぶこと。
fn ensure_workspace_members_table(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        let _ = db.execute(
            "CREATE TABLE IF NOT EXISTS workspace_members(\
             workspace_id TEXT NOT NULL, user_id TEXT NOT NULL, \
             role TEXT NOT NULL DEFAULT 'member', added_at TEXT, \
             PRIMARY KEY(workspace_id, user_id))",
            &[],
        );
    });
}

/// `users.last_login` 列を後付けする(使われていないアカウントを見分けるため)。
fn ensure_user_last_login_column(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        let _ = db.execute("ALTER TABLE users ADD COLUMN last_login TEXT", &[]);
    });
}

/// `send_history.user_id` 列を後付けする(どの担当者の送信かを残すため)。
///
/// 1社複数ユーザーにする以上、これが無いと人数だけ増えて**誰が送ったか追えない**。
/// 既存行は NULL のまま(遡って埋められないので偽らない)。
fn ensure_send_history_user_column(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        let _ = db.execute("ALTER TABLE send_history ADD COLUMN user_id TEXT", &[]);
    });
}

/// `run_logs.platform` 列を後付けする(どの媒体での出来事かを残すため)。
///
/// run_logs は媒体列を持たずに作られている。これが無いと
/// 「AMBI だけ件名の文字数で弾かれている」のような**媒体単位の切り分けができない**。
/// 既に列がある/ALTER 非対応でも失敗を無視する(冪等)。※同期。spawn_blocking 内で呼ぶこと。
fn ensure_run_log_platform_column(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        // 既に列が存在すると Turso はエラーを返すが、それは正常系として無視する。
        let _ = db.execute(RUN_LOG_PLATFORM_ALTER_SQL, &[]);
    });
}

/// `run_logs` に失敗の中身を残す4列を後付けする。
///
/// `status` だけでは「verify_failed が何件」までしか言えず、**なぜ落ちたか**が
/// 中央に残らない。error_kind(分類)/error_step(どの段で)/error_detail(画面の文言)/
/// error_url(現場の URL)を足して、顧客PCのログを見に行かずに原因を言えるようにする。
/// SQLite の ALTER TABLE は1文で1列しか足せないので4回実行する。
/// 既に列がある/ALTER 非対応でも失敗を無視する(冪等)。※同期。spawn_blocking 内で呼ぶこと。
fn ensure_run_log_error_columns(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        for sql in RUN_LOG_ERROR_COLUMN_ALTER_SQL {
            // 既に列が存在する場合のエラーは正常系として無視する。
            let _ = db.execute(sql, &[]);
        }
    });
}

/// このユーザーが入る workspace と、そこでの役割を解決する。
///
/// 所有者であるものと、メンバーであるものの**両方**を見る。所有者を優先して返す。
/// 所有者も拾うのは互換のため: `workspace_members` は後から足した表なので、
/// 既存顧客は所有者行しか持たない。members だけを見ると**既存顧客が全員
/// ログインできなくなる**。
///
/// 役割も workspace ごとに決まる。`users.role` だけを見ると、role 列を後付けした
/// 時点で既に居た所有者が既定値の 'member' になり、**その会社の管理者が
/// 担当者管理をできなくなる**(ローカルアプリ側の実装で実際に踏んだ)。
/// ※同期。spawn_blocking 内で呼ぶこと。
fn resolve_workspace_for_user(db: &TursoDb, user_id: &str) -> Result<(String, String), CoreErr> {
    ensure_workspace_members_table(db);
    let uid = user_id.to_string();
    let p: [&dyn ToSqlTurso; 4] = [&uid, &uid, &uid, &uid];
    let rows = db
        .query(
            "SELECT w.id AS id, \
             CASE WHEN w.owner_user_id=? THEN 1 ELSE 0 END AS is_owner, \
             COALESCE(m.role, CASE WHEN w.owner_user_id=? THEN 'master' END, 'member') AS role \
             FROM workspaces w \
             LEFT JOIN workspace_members m ON m.workspace_id=w.id AND m.user_id=? \
             WHERE w.owner_user_id=? OR m.user_id IS NOT NULL \
             ORDER BY is_owner DESC, w.created_at LIMIT 1",
            &p,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    match rows.first() {
        Some(r) => Ok((get_str(r, "id"), get_str(r, "role"))),
        None => Ok((String::new(), "member".to_string())),
    }
}

/// kill_switches を参照し、global もしくは当該 workspace の送信が無効化されているか判定。
/// ローカルアプリ側 SqliteRepository.is_sending_disabled と同ロジック。※同期。
fn sending_disabled(db: &TursoDb, workspace_id: &str) -> Result<(bool, String), CoreErr> {
    let wid = workspace_id.to_string();
    let global = "global".to_string();
    let params: [&dyn ToSqlTurso; 2] = [&global, &wid];
    let rows = db
        .query(
            "SELECT scope,disabled,reason FROM kill_switches WHERE scope IN (?,?) AND disabled=1",
            &params,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    match rows.first() {
        Some(r) => Ok((true, get_str(r, "reason"))),
        None => Ok((false, String::new())),
    }
}

// ===== エンドポイント（薄いasyncラッパ + 同期core） =====

async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "scout-backend",
        "db_connected": state.scout_db.is_some(),
    }))
}

async fn login(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> ApiResult {
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || login_core(&dbh, email, password)).await
}

fn login_core(db: &TursoDb, email: String, password: String) -> Result<Value, CoreErr> {
    if email.is_empty() || password.is_empty() {
        return Err(cerr(
            StatusCode::BAD_REQUEST,
            "メールとパスワードが必要です",
        ));
    }
    // 解約遮断用の disabled 列・権限用の role 列を用意(一度だけ ALTER)。SELECT 前に必須。
    ensure_user_disabled_column(db);
    ensure_user_role_column(db);
    let params: [&dyn ToSqlTurso; 1] = [&email];
    let rows = db
        .query(
            "SELECT id,email,password_hash,name,disabled,COALESCE(role,'member') AS role \
             FROM users WHERE email=?",
            &params,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let row = rows
        .first()
        .ok_or_else(|| cerr(StatusCode::UNAUTHORIZED, "メールまたはパスワードが違います"))?;
    let hash = get_str(row, "password_hash");
    if !bcrypt::verify(&password, &hash).unwrap_or(false) {
        return Err(cerr(
            StatusCode::UNAUTHORIZED,
            "メールまたはパスワードが違います",
        ));
    }
    // パスワード検証成功後に無効化チェック(解約済みアカウントを遮断)。
    let disabled = row.get("disabled").and_then(|v| v.as_i64()).unwrap_or(0);
    if disabled != 0 {
        return Err(cerr(
            StatusCode::FORBIDDEN,
            "アカウントが無効化されています",
        ));
    }
    let user_id = get_str(row, "id");
    let name = get_str(row, "name");

    // 所有者としてだけでなく、担当者(workspace_members)としても workspace を解決する。
    // 以前は `WHERE owner_user_id=?` だけを見ていたため、担当者を追加しても
    // ログインがそれを無視し、1社複数ユーザーが成立しなかった。
    // 役割もここで決まる(所有者=master、メンバー=members 行の role)。
    let (workspace_id, role) = resolve_workspace_for_user(db, &user_id)?;

    // 最終ログイン。使われていないアカウントを見分ける唯一の手がかり。
    // 記録できないことはログインを拒む理由にならないので、失敗は無視する。
    ensure_user_last_login_column(db);
    let now_login = now_str();
    let pl: [&dyn ToSqlTurso; 2] = [&now_login, &user_id];
    let _ = db.execute("UPDATE users SET last_login=? WHERE id=?", &pl);

    let token = new_token();
    let created = now_str();
    let expires = (Utc::now() + Duration::days(SESSION_TTL_DAYS))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let ip: [&dyn ToSqlTurso; 7] = [
        &token,
        &user_id,
        &email,
        &name,
        &workspace_id,
        &created,
        &expires,
    ];
    db.execute(
        "INSERT INTO auth_sessions(token,user_id,email,name,workspace_id,created_at,expires_at) VALUES(?,?,?,?,?,?,?)",
        &ip,
    )
    .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;

    Ok(json!({
        "ok": true,
        "token": token,
        "user": {"email": email, "name": name, "workspace_id": workspace_id, "role": role},
    }))
}

async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        Ok(json!({
            "user": {"user_id": u.user_id, "email": u.email, "name": u.name, "workspace_id": u.workspace_id, "role": u.role}
        }))
    })
    .await
}

async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        if !token.is_empty() {
            let p: [&dyn ToSqlTurso; 1] = [&token];
            let _ = dbh.execute("DELETE FROM auth_sessions WHERE token=?", &p);
        }
        Ok(json!({ "ok": true }))
    })
    .await
}

async fn get_config(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        let key = CONFIG_KEY.to_string();
        let p: [&dyn ToSqlTurso; 2] = [&u.workspace_id, &key];
        let rows = dbh
            .query(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                &p,
            )
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let cfg: Value = match rows.first().map(|r| get_str(r, "value")) {
            Some(s) if !s.is_empty() => {
                serde_json::from_str(&s).unwrap_or(json!({"campaigns": []}))
            }
            _ => serde_json::from_str(DEFAULT_CONFIG_JSON).unwrap_or(json!({"campaigns": []})),
        };
        Ok(json!({ "config": cfg }))
    })
    .await
}

async fn save_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let cfg_str = match body.get("config") {
        Some(c) => match serde_json::to_string(c) {
            Ok(s) => s,
            Err(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"config が不正です"})),
                ))
            }
        },
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"config が必要です"})),
            ))
        }
    };
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        let key = CONFIG_KEY.to_string();
        let p: [&dyn ToSqlTurso; 3] = [&u.workspace_id, &key, &cfg_str];
        dbh.execute(
            "INSERT OR REPLACE INTO kv_settings(workspace_id,key,value) VALUES(?,?,?)",
            &p,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        Ok(json!({ "ok": true }))
    })
    .await
}

fn require_credentials_token(token: &str) -> Result<(), CoreErr> {
    if token.is_empty() {
        return Err(cerr(StatusCode::UNAUTHORIZED, "authentication required"));
    }
    Ok(())
}

fn config_with_sections(sections: &Value) -> Result<Value, CoreErr> {
    let updates = sections
        .as_object()
        .ok_or_else(|| cerr(StatusCode::BAD_REQUEST, "sections はobjectが必要です"))?;
    let mut config: Value = serde_json::from_str(DEFAULT_CONFIG_JSON).map_err(|e| {
        cerr(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("既定config不正: {e}"),
        )
    })?;
    let target = config.as_object_mut().ok_or_else(|| {
        cerr(
            StatusCode::INTERNAL_SERVER_ERROR,
            "既定configがobjectではありません",
        )
    })?;
    for (key, value) in updates {
        target.insert(key.clone(), value.clone());
    }
    Ok(config)
}

/// config の指定された最上位セクションだけを原子的に更新する。
/// キャンペーン設定の保存が prompt_templates / resend_templates を消す競合を防ぐ。
async fn patch_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let sections = match body.get("sections") {
        Some(value) if value.is_object() => value.clone(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"sections はobjectが必要です"})),
            ))
        }
    };
    let patch_str = serde_json::to_string(&sections).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"sections が不正です"})),
        )
    })?;
    let initial_str = serde_json::to_string(
        &config_with_sections(&sections)
            .map_err(|(code, message)| (code, Json(json!({"error": message}))))?,
    )
    .map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"sections が不正です"})),
        )
    })?;
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        let key = CONFIG_KEY.to_string();
        let p: [&dyn ToSqlTurso; 4] = [&u.workspace_id, &key, &initial_str, &patch_str];
        dbh.execute(CONFIG_PATCH_SQL, &p)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;

        let q: [&dyn ToSqlTurso; 2] = [&u.workspace_id, &key];
        let rows = dbh
            .query(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                &q,
            )
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let value = rows
            .first()
            .map(|row| get_str(row, "value"))
            .ok_or_else(|| {
                cerr(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "更新後configがありません",
                )
            })?;
        let config = serde_json::from_str::<Value>(&value).map_err(|e| {
            cerr(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新後config不正: {e}"),
            )
        })?;
        Ok(json!({ "ok": true, "config": config }))
    })
    .await
}

#[derive(Serialize, Deserialize)]
struct StoredCredentials {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct CredentialEnvelope {
    version: u8,
    nonce: String,
    ciphertext: String,
}

fn valid_platform_slug(platform: &str) -> bool {
    let bytes = platform.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn require_valid_platform_slug(platform: &str) -> Result<(), CoreErr> {
    if !valid_platform_slug(platform) {
        return Err(cerr(StatusCode::BAD_REQUEST, "invalid platform slug"));
    }
    Ok(())
}

fn credential_storage_key(platform: &str) -> String {
    format!("{CREDENTIAL_KEY_PREFIX}{platform}")
}

fn credential_aad(workspace_id: &str, platform: &str) -> Vec<u8> {
    format!("scout-credentials:v1\0{workspace_id}\0{platform}").into_bytes()
}

fn decode_credentials_key(encoded: &str) -> Result<[u8; 32], ()> {
    let decoded = BASE64_STANDARD.decode(encoded.trim()).map_err(|_| ())?;
    decoded.try_into().map_err(|_| ())
}

fn credentials_key_from_encoded(encoded: Option<&str>) -> Result<[u8; 32], CoreErr> {
    let encoded = encoded.ok_or_else(|| {
        cerr(
            StatusCode::SERVICE_UNAVAILABLE,
            "credential encryption is unavailable",
        )
    })?;
    decode_credentials_key(encoded).map_err(|_| {
        cerr(
            StatusCode::SERVICE_UNAVAILABLE,
            "credential encryption is unavailable",
        )
    })
}

fn load_credentials_key() -> Result<[u8; 32], CoreErr> {
    let encoded = std::env::var(SCOUT_CREDENTIALS_KEY_ENV).ok();
    credentials_key_from_encoded(encoded.as_deref())
}

fn encrypt_credentials(
    key: &[u8; 32],
    workspace_id: &str,
    platform: &str,
    credentials: &StoredCredentials,
) -> Result<String, ()> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| ())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let plaintext = serde_json::to_vec(credentials).map_err(|_| ())?;
    let aad = credential_aad(workspace_id, platform);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: &plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| ())?;
    let envelope = CredentialEnvelope {
        version: 1,
        nonce: BASE64_STANDARD.encode(nonce),
        ciphertext: BASE64_STANDARD.encode(ciphertext),
    };
    serde_json::to_string(&envelope).map_err(|_| ())
}

fn decrypt_credentials(
    key: &[u8; 32],
    workspace_id: &str,
    platform: &str,
    stored: &str,
) -> Result<StoredCredentials, ()> {
    let envelope: CredentialEnvelope = serde_json::from_str(stored).map_err(|_| ())?;
    if envelope.version != 1 {
        return Err(());
    }
    let nonce = BASE64_STANDARD.decode(envelope.nonce).map_err(|_| ())?;
    if nonce.len() != 12 {
        return Err(());
    }
    let ciphertext = BASE64_STANDARD
        .decode(envelope.ciphertext)
        .map_err(|_| ())?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| ())?;
    let aad = credential_aad(workspace_id, platform);
    let plaintext = cipher
        .decrypt(
            aes_gcm::Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| ())?;
    serde_json::from_slice(&plaintext).map_err(|_| ())
}

fn credential_status_value(credentials: &StoredCredentials) -> Value {
    json!({
        "username": credentials.username,
        "password_set": !credentials.password.is_empty()
    })
}

fn platform_credentials_value(platform: &str, credentials: &StoredCredentials) -> Value {
    json!({
        "credential": {
            "platform": platform,
            "username": credentials.username,
            "password": credentials.password
        }
    })
}

async fn save_platform_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> CredentialApiResult {
    let token = token_from(&headers);
    let platform = body
        .get("platform")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let username = body
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let password = body
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let dbh = match take_db(&state) {
        Ok(db) => db,
        Err((code, message)) => {
            return Err((
                code,
                credential_response_headers(),
                Json(json!({ "error": message })),
            ))
        }
    };
    run_credentials(move || {
        let user = require_credentials_user(&dbh, &token)?;
        require_valid_platform_slug(&platform)?;
        if username.is_empty() || password.is_empty() {
            return Err(cerr(
                StatusCode::BAD_REQUEST,
                "username and password are required",
            ));
        }
        let encryption_key = load_credentials_key()?;
        let credentials = StoredCredentials { username, password };
        let encrypted =
            encrypt_credentials(&encryption_key, &user.workspace_id, &platform, &credentials)
                .map_err(|_| {
                    cerr(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "credential encryption failed",
                    )
                })?;
        let storage_key = credential_storage_key(&platform);
        let params: [&dyn ToSqlTurso; 3] = [&user.workspace_id, &storage_key, &encrypted];
        dbh.execute(CREDENTIAL_UPSERT_SQL, &params).map_err(|_| {
            cerr(
                StatusCode::INTERNAL_SERVER_ERROR,
                "credential storage failed",
            )
        })?;
        Ok(json!({
            "ok": true,
            "platform": platform,
            "username": credentials.username,
            "password_set": true
        }))
    })
    .await
}

async fn get_credentials_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> CredentialApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(db) => db,
        Err((code, message)) => {
            return Err((
                code,
                credential_response_headers(),
                Json(json!({ "error": message })),
            ))
        }
    };
    run_credentials(move || {
        let user = require_credentials_user(&dbh, &token)?;
        let encryption_key = load_credentials_key()?;
        let pattern = format!("{CREDENTIAL_KEY_PREFIX}*");
        let params: [&dyn ToSqlTurso; 2] = [&user.workspace_id, &pattern];
        let rows = dbh
            .query(
                "SELECT key,value FROM kv_settings WHERE workspace_id=? AND key GLOB ?",
                &params,
            )
            .map_err(|_| {
                cerr(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "credential storage failed",
                )
            })?;
        let mut statuses = Map::new();
        for row in rows {
            let storage_key = get_str(&row, "key");
            let Some(platform) = storage_key.strip_prefix(CREDENTIAL_KEY_PREFIX) else {
                continue;
            };
            if !valid_platform_slug(platform) {
                continue;
            }
            let stored = get_str(&row, "value");
            if let Ok(credentials) =
                decrypt_credentials(&encryption_key, &user.workspace_id, platform, &stored)
            {
                statuses.insert(platform.to_string(), credential_status_value(&credentials));
            }
        }
        Ok(json!({ "credentials": statuses }))
    })
    .await
}

async fn get_platform_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(platform): Path<String>,
) -> CredentialApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(db) => db,
        Err((code, message)) => {
            return Err((
                code,
                credential_response_headers(),
                Json(json!({ "error": message })),
            ))
        }
    };
    run_credentials(move || {
        let user = require_credentials_user(&dbh, &token)?;
        require_valid_platform_slug(&platform)?;
        let encryption_key = load_credentials_key()?;
        let storage_key = credential_storage_key(&platform);
        let params: [&dyn ToSqlTurso; 2] = [&user.workspace_id, &storage_key];
        let rows = dbh
            .query(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                &params,
            )
            .map_err(|_| {
                cerr(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "credential storage failed",
                )
            })?;
        let stored = rows
            .first()
            .map(|row| get_str(row, "value"))
            .ok_or_else(|| cerr(StatusCode::NOT_FOUND, "credentials not found"))?;
        let credentials =
            decrypt_credentials(&encryption_key, &user.workspace_id, &platform, &stored).map_err(
                |_| {
                    cerr(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "credential data is unavailable",
                    )
                },
            )?;
        Ok(platform_credentials_value(&platform, &credentials))
    })
    .await
}

async fn get_state(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        let key = STATE_KEY.to_string();
        let p: [&dyn ToSqlTurso; 2] = [&u.workspace_id, &key];
        let rows = dbh
            .query(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                &p,
            )
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let st: Value = match rows.first().map(|r| get_str(r, "value")) {
            Some(s) if !s.is_empty() => {
                serde_json::from_str(&s).unwrap_or(json!({"campaigns": {}, "sessions": []}))
            }
            _ => json!({"campaigns": {}, "sessions": []}),
        };
        Ok(json!({ "state": st }))
    })
    .await
}

async fn save_state(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let st_str = match body.get("state") {
        Some(s) => match serde_json::to_string(s) {
            Ok(v) => v,
            Err(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"state が不正です"})),
                ))
            }
        },
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"state が必要です"})),
            ))
        }
    };
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        let key = STATE_KEY.to_string();
        let p: [&dyn ToSqlTurso; 3] = [&u.workspace_id, &key, &st_str];
        dbh.execute(
            "INSERT OR REPLACE INTO kv_settings(workspace_id,key,value) VALUES(?,?,?)",
            &p,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        Ok(json!({ "ok": true }))
    })
    .await
}

// ===== 送信の安全装置（冪等ガード・キルスイッチ・解約遮断） =====

/// 送信記録の登録（冪等）。同一(workspace,campaign,candidate)が既にあれば挿入しない。
/// 不可逆なスカウト送信の二重送信を中央側で防ぐ最後の砦。要トークン。
async fn sent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let campaign_id = body
        .get("campaign_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let candidate = body
        .get("candidate_web_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let platform = body
        .get("platform")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let subject_chars = body
        .get("subject_chars")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let body_chars = body.get("body_chars").and_then(|v| v.as_i64()).unwrap_or(0);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        if campaign_id.is_empty() || candidate.is_empty() {
            return Err(cerr(StatusCode::BAD_REQUEST, "campaign_id と candidate_web_id が必要です"));
        }
        // 冪等: 既に同一(workspace,campaign,candidate)の送信履歴があれば挿入しない。
        let pe: [&dyn ToSqlTurso; 3] = [&u.workspace_id, &campaign_id, &candidate];
        let existing = dbh
            .query(
                "SELECT 1 AS x FROM send_history WHERE workspace_id=? AND campaign_id=? AND candidate_web_id=?",
                &pe,
            )
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        if !existing.is_empty() {
            return Ok(json!({ "ok": true, "already": true }));
        }
        let now = now_str();
        // どの担当者の送信か。1社複数ユーザーでは、これが無いと人数だけ増えて
        // 「誰が送ったか」を後から示せない。
        ensure_send_history_user_column(&dbh);
        let pi: [&dyn ToSqlTurso; 8] = [
            &campaign_id,
            &candidate,
            &platform,
            &now,
            &subject_chars,
            &body_chars,
            &u.workspace_id,
            &u.user_id,
        ];
        dbh.execute(
            "INSERT INTO send_history(campaign_id,candidate_web_id,platform,sent_at,subject_chars,body_chars,workspace_id,user_id) VALUES(?,?,?,?,?,?,?,?)",
            &pi,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        Ok(json!({ "ok": true, "already": false }))
    })
    .await
}

/// 送信済み判定。ローカルアプリが送信前に叩き二重送信を回避する。要トークン。
async fn has_sent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    let token = token_from(&headers);
    let campaign_id = q.get("campaign_id").cloned().unwrap_or_default();
    let web_id = q.get("web_id").cloned().unwrap_or_default();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        let p: [&dyn ToSqlTurso; 3] = [&u.workspace_id, &campaign_id, &web_id];
        let rows = dbh
            .query(
                "SELECT 1 AS x FROM send_history WHERE workspace_id=? AND campaign_id=? AND candidate_web_id=?",
                &p,
            )
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        Ok(json!({ "sent": !rows.is_empty() }))
    })
    .await
}

/// 送信実績の集計。呼び出し元ユーザーの workspace に絞って send_history を集計する。
/// 全ユーザー・全端末の実送信が中央 send_history に集まるため、これが workspace 全体の
/// 送付数の正本になる(端末ローカルの CSV/jsonl は1台分のデバッグ用)。要トークン。
/// 返却: total(総送付数)/ by_platform(媒体別)/ by_campaign(キャンペーン別・id)/
///       by_day(日別・YYYY-MM-DD)/ recent(直近50件)。キャンペーン名の解決は呼び出し側で config を使う。
async fn stats(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        let ws: [&dyn ToSqlTurso; 1] = [&u.workspace_id];
        let q = |sql: &str| {
            dbh.query(sql, &ws)
                .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))
        };
        // 総送付数
        let total = q("SELECT COUNT(*) AS cnt FROM send_history WHERE workspace_id=?")?
            .first()
            .and_then(|r| r.get("cnt").and_then(|v| v.as_i64()))
            .unwrap_or(0);
        // 媒体別
        let by_platform: Vec<Value> = q(
            "SELECT platform, COUNT(*) AS cnt FROM send_history WHERE workspace_id=? \
             GROUP BY platform ORDER BY cnt DESC",
        )?
        .iter()
        .map(|r| json!({ "platform": get_str(r, "platform"), "cnt": r.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0) }))
        .collect();
        // キャンペーン別(id。名前は呼び出し側で config から解決)
        let by_campaign: Vec<Value> = q(
            "SELECT campaign_id, COUNT(*) AS cnt FROM send_history WHERE workspace_id=? \
             GROUP BY campaign_id ORDER BY cnt DESC",
        )?
        .iter()
        .map(|r| json!({ "campaign_id": get_str(r, "campaign_id"), "cnt": r.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0) }))
        .collect();
        // 日別(sent_at の先頭10文字= YYYY-MM-DD)
        let by_day: Vec<Value> = q(
            "SELECT substr(sent_at,1,10) AS day, COUNT(*) AS cnt FROM send_history WHERE workspace_id=? \
             GROUP BY day ORDER BY day",
        )?
        .iter()
        .map(|r| json!({ "day": get_str(r, "day"), "cnt": r.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0) }))
        .collect();
        // 直近送信(50件)
        let recent: Vec<Value> = q(
            "SELECT campaign_id, candidate_web_id, platform, sent_at FROM send_history WHERE workspace_id=? \
             ORDER BY sent_at DESC LIMIT 50",
        )?
        .iter()
        .map(|r| json!({
            "campaign_id": get_str(r, "campaign_id"),
            "candidate_web_id": get_str(r, "candidate_web_id"),
            "platform": get_str(r, "platform"),
            "sent_at": get_str(r, "sent_at"),
        }))
        .collect();
        Ok(json!({
            "total": total,
            "by_platform": by_platform,
            "by_campaign": by_campaign,
            "by_day": by_day,
            "recent": recent,
        }))
    })
    .await
}

// ===== 実行ログ（送れなかった候補者の結末と理由） =====
//
// `send_history` は「送れた分」だけの表で、スキップ・失敗とその理由は中央に
// 1行も残っていなかった(`run_logs` が 0 行)。ここで全候補者の結末を受け、
// CSV で取り出せるようにする。

/// 実行ログ1行の記録。**送信成功に限らず**スキップ・失敗も受ける。要トークン。
///
/// workspace は**セッションから**決める(ボディの値は見ない)。ログなので冪等化せず、
/// 来た行は全部入れる(同じ候補者が3回弾かれたら3行残るのが正しい)。
/// `candidate_web_id` は空でも受ける(ID を読む前に落ちる失敗が実際にあるため)。
async fn run_log(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let ts_in = body_str(&body, "ts");
    let campaign_id = body_str(&body, "campaign_id");
    let candidate = body_str(&body, "candidate_web_id");
    let platform = body_str(&body, "platform");
    let status = body_str(&body, "status");
    let error_kind = body_str(&body, "error_kind");
    let error_step = body_str(&body, "error_step");
    // 暴走したページの文面で表が膨らまないよう、サーバ側で切る(入口で止める)。
    let error_detail = clip_chars(
        &body_str(&body, "error_detail"),
        RUN_LOG_ERROR_DETAIL_MAX_CHARS,
    );
    let error_url = clip_chars(&body_str(&body, "error_url"), RUN_LOG_ERROR_URL_MAX_CHARS);
    let iteration = body.get("iteration").and_then(|v| v.as_i64()).unwrap_or(0);
    let subject_chars = body
        .get("subject_chars")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let body_chars = body.get("body_chars").and_then(|v| v.as_i64()).unwrap_or(0);
    let api_seconds = body
        .get("api_seconds")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        if campaign_id.is_empty() || status.is_empty() {
            return Err(cerr(
                StatusCode::BAD_REQUEST,
                "campaign_id と status が必要です",
            ));
        }
        ensure_run_log_platform_column(&dbh);
        ensure_run_log_error_columns(&dbh);
        // ts 省略時はサーバ時刻。空文字のまま入れると日付で絞れない行になる。
        let ts = if ts_in.is_empty() { now_str() } else { ts_in };
        let p: [&dyn ToSqlTurso; 15] = [
            &ts,
            &campaign_id,
            &iteration,
            &candidate,
            &subject_chars,
            &body_chars,
            &api_seconds,
            &status,
            &u.workspace_id,
            &u.user_id,
            &platform,
            &error_kind,
            &error_step,
            &error_detail,
            &error_url,
        ];
        dbh.execute(RUN_LOG_INSERT_SQL, &p)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        Ok(json!({ "ok": true }))
    })
    .await
}

/// CSV セル1個。カンマ・引用符・改行を含むときだけ引用符で囲む(RFC 4180)。
/// `error_detail` には画面の文言がそのまま入るので、ここを通さないと列がずれる。
fn run_log_csv_cell(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 数値セル。NULL・列なしは **0 ではなく空欄**にする
/// (「件名0文字」と「記録が無い」を取り違えないため)。
fn run_log_num_cell(row: &HashMap<String, Value>, key: &str) -> String {
    match row.get(key) {
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// 1行ぶんの CSV。列の順序は `RUN_LOG_CSV_HEADER` と一致させる。
fn run_log_csv_row(row: &HashMap<String, Value>) -> String {
    let text = ["ts", "campaign_id", "campaign_name", "platform", "candidate_web_id", "status"];
    let mut cells: Vec<String> = text
        .iter()
        .map(|k| run_log_csv_cell(&get_str(row, k)))
        .collect();
    cells.push(run_log_num_cell(row, "subject_chars"));
    cells.push(run_log_num_cell(row, "body_chars"));
    cells.push(run_log_num_cell(row, "api_seconds"));
    for k in ["error_kind", "error_step", "error_detail", "error_url", "user_id"] {
        cells.push(run_log_csv_cell(&get_str(row, k)));
    }
    cells.join(",")
}

/// 取得行から CSV 全文を組む。戻りは (CSV, 打ち切ったか)。
///
/// 上限を超えた分は**黙って捨てない**。最終行に注記を1行足し、ヘッダ側でも知らせる。
fn build_run_log_csv(rows: &[HashMap<String, Value>]) -> (String, bool) {
    let truncated = rows.len() > RUN_LOG_EXPORT_MAX_ROWS;
    let mut out = String::new();
    out.push('\u{feff}'); // BOM。Windows の Excel が UTF-8 と判定できるようにする。
    out.push_str(RUN_LOG_CSV_HEADER);
    out.push_str("\r\n");
    for row in rows.iter().take(RUN_LOG_EXPORT_MAX_ROWS) {
        out.push_str(&run_log_csv_row(row));
        out.push_str("\r\n");
    }
    if truncated {
        out.push_str(&run_log_csv_cell(RUN_LOG_TRUNCATED_NOTICE));
        out.push_str(&",".repeat(RUN_LOG_CSV_COLUMNS - 1));
        out.push_str("\r\n");
    }
    (out, truncated)
}

fn run_log_csv_headers(truncated: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"run_logs.csv\""),
    );
    // 打ち切りの有無は毎回出す。true のときだけ付けると、ヘッダが無い理由が
    // 「打ち切っていない」のか「古い版のサーバ」なのか呼び出し元から区別できない。
    headers.insert(
        "x-truncated",
        HeaderValue::from_static(if truncated { "true" } else { "false" }),
    );
    headers
}

/// 実行ログの CSV エクスポート。要トークン。呼び出し元の workspace だけに絞る。
///
/// クエリ: `from`/`to`(`YYYY-MM-DD`、任意)、`platform`(任意)。いずれも省略可。
/// 返却: UTF-8 BOM 付き CSV(`text/csv`、`run_logs.csv` として添付)。
/// 50,000 行で打ち切り、そのときは `X-Truncated: true` と最終行の注記で明示する。
async fn run_log_export(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> CsvApiResult {
    let token = token_from(&headers);
    let pick = |k: &str| q.get(k).map(|v| v.trim().to_string()).unwrap_or_default();
    let from = pick("from");
    let to = pick("to");
    let platform = pick("platform");
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    let built = tokio::task::spawn_blocking(move || -> Result<(String, bool), CoreErr> {
        let u = require_user(&dbh, &token)?;
        // 列が無い DB では SELECT 自体が落ちるので、読む前に後付けを済ませる。
        ensure_run_log_platform_column(&dbh);
        ensure_run_log_error_columns(&dbh);
        let has_campaigns = !dbh
            .query(RUN_LOG_CAMPAIGNS_EXISTS_SQL, &[])
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?
            .is_empty();
        let sql = if has_campaigns {
            RUN_LOG_EXPORT_SQL
        } else {
            RUN_LOG_EXPORT_SQL_WITHOUT_CAMPAIGNS
        };
        let p: [&dyn ToSqlTurso; 7] = [
            &u.workspace_id,
            &from,
            &from,
            &to,
            &to,
            &platform,
            &platform,
        ];
        let rows = dbh
            .query(sql, &p)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        Ok(build_run_log_csv(&rows))
    })
    .await;
    match built {
        Ok(Ok((csv, truncated))) => Ok((run_log_csv_headers(truncated), csv)),
        Ok(Err((code, msg))) => Err((code, Json(json!({ "error": msg })))),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal task error" })),
        )),
    }
}

/// キルスイッチ状態の照会。ローカルアプリが送信ループ前に確認する。要トークン。
async fn killswitch(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let u = require_user(&dbh, &token)?;
        let (disabled, reason) = sending_disabled(&dbh, &u.workspace_id)?;
        Ok(json!({ "disabled": disabled, "reason": reason }))
    })
    .await
}

/// キルスイッチの設定（管理者）。env `SCOUT_ADMIN_TOKEN` を持つ管理者のみ。
async fn admin_killswitch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let admin_token = std::env::var("SCOUT_ADMIN_TOKEN").unwrap_or_default();
    let provided = headers.get("x-admin-token").and_then(|v| v.to_str().ok());
    if admin_token.is_empty() || provided != Some(admin_token.as_str()) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error":"管理者トークンが必要です"})),
        ));
    }
    // scope 省略時は 'global'（全体停止）。
    let scope = {
        let s = body
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if s.is_empty() {
            "global".to_string()
        } else {
            s
        }
    };
    let disabled = if body
        .get("disabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
    {
        1i64
    } else {
        0i64
    };
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let now = now_str();
        let p: [&dyn ToSqlTurso; 4] = [&scope, &disabled, &reason, &now];
        dbh.execute(
            "INSERT OR REPLACE INTO kill_switches(scope,disabled,reason,updated_at) VALUES(?,?,?,?)",
            &p,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        Ok(json!({ "ok": true, "scope": scope, "disabled": disabled != 0 }))
    })
    .await
}

/// アカウント無効化（解約遮断）。管理者のみ。無効化時は既存セッションも即失効。
async fn admin_disable(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let admin_token = std::env::var("SCOUT_ADMIN_TOKEN").unwrap_or_default();
    let provided = headers.get("x-admin-token").and_then(|v| v.to_str().ok());
    if admin_token.is_empty() || provided != Some(admin_token.as_str()) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error":"管理者トークンが必要です"})),
        ));
    }
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let disabled = if body
        .get("disabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
    {
        1i64
    } else {
        0i64
    };
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        if email.is_empty() {
            return Err(cerr(StatusCode::BAD_REQUEST, "email が必要です"));
        }
        // disabled 列を用意してから更新（未追加環境でも UPDATE が通るように）。
        ensure_user_disabled_column(&dbh);
        let pu: [&dyn ToSqlTurso; 2] = [&disabled, &email];
        dbh.execute("UPDATE users SET disabled=? WHERE email=?", &pu)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        // 無効化時は当該ユーザーの既存セッションを全削除して即失効させる。
        if disabled != 0 {
            let pd: [&dyn ToSqlTurso; 1] = [&email];
            let _ = dbh.execute("DELETE FROM auth_sessions WHERE email=?", &pd);
        }
        Ok(json!({ "ok": true, "email": email, "disabled": disabled != 0 }))
    })
    .await
}

/// 管理者プロビジョニング（招待制）。env `SCOUT_ADMIN_TOKEN` を持つ管理者のみ。
async fn provision(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let admin_token = std::env::var("SCOUT_ADMIN_TOKEN").unwrap_or_default();
    let provided = headers.get("x-admin-token").and_then(|v| v.to_str().ok());
    if admin_token.is_empty() || provided != Some(admin_token.as_str()) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error":"管理者トークンが必要です"})),
        ));
    }
    let company = body
        .get("company")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name_in = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let name = if name_in.is_empty() {
        company.clone()
    } else {
        name_in
    };
    // role は master/member のみ許可。既定は member(招待された実務者)。master 昇格は明示指定。
    let role = match body
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("member")
        .trim()
    {
        "master" => "master".to_string(),
        _ => "member".to_string(),
    };
    // 既存の会社に担当者を足す場合だけ指定する。空なら新しい会社として作る。
    let workspace_id = body
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || provision_core(&dbh, company, email, password, name, role, workspace_id)).await
}

/// 顧客(会社)アカウントを作る。`workspace_id` を渡すと**既存の会社に担当者を追加**する。
///
/// `workspace_id` が空なら従来どおり新しい会社として workspace を作り、その人を master にする。
///
/// **既存 workspace に足すときに新しい workspace を作ってはいけない。**
/// `daily_counters` と `send_history` はどちらも workspace 単位なので、同じ会社が
/// 2つの workspace を持つと日次上限が実質2倍になり、**同じ候補者へ2通送られる**。
/// 取り消せない事故なので、存在しない workspace_id は作らずに 404 で返す。
fn provision_core(
    db: &TursoDb,
    company: String,
    email: String,
    password: String,
    name: String,
    role: String,
    workspace_id: String,
) -> Result<Value, CoreErr> {
    if company.is_empty() || email.is_empty() || password.len() < 8 {
        return Err(cerr(
            StatusCode::BAD_REQUEST,
            "company・email・8文字以上のpassword が必要です",
        ));
    }
    ensure_user_role_column(db);
    let pe: [&dyn ToSqlTurso; 1] = [&email];
    let exists = db
        .query("SELECT 1 AS x FROM users WHERE email=?", &pe)
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    if !exists.is_empty() {
        return Err(cerr(StatusCode::CONFLICT, "既に登録済みのメールです"));
    }

    ensure_workspace_members_table(db);
    let user_id = new_id();
    let now = now_str();
    let hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST)
        .map_err(|_| cerr(StatusCode::INTERNAL_SERVER_ERROR, "ハッシュ生成失敗"))?;

    let joining = !workspace_id.trim().is_empty();
    let ws_id = if joining {
        // 既存 workspace への追加。**存在しない id で新規作成しない**
        // (作ると会社が2つの workspace を持ち、上限が実質2倍になる)。
        let wid = workspace_id.trim().to_string();
        let pq: [&dyn ToSqlTurso; 1] = [&wid];
        let found = db
            .query("SELECT id FROM workspaces WHERE id=?", &pq)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        if found.is_empty() {
            return Err(cerr(
                StatusCode::NOT_FOUND,
                "指定された workspace がありません",
            ));
        }
        wid
    } else {
        let wid = new_id();
        let pw: [&dyn ToSqlTurso; 4] = [&wid, &company, &user_id, &now];
        db.execute(
            "INSERT INTO workspaces(id,name,owner_user_id,created_at) VALUES(?,?,?,?)",
            &pw,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        wid
    };

    // 新しい会社の1人目は master。member だと担当者を追加できる人が誰も居なくなる。
    let effective_role = if joining {
        role.clone()
    } else {
        "master".to_string()
    };

    let pu: [&dyn ToSqlTurso; 6] = [&user_id, &email, &hash, &name, &now, &effective_role];
    db.execute(
        "INSERT INTO users(id,email,password_hash,name,created_at,role) VALUES(?,?,?,?,?,?)",
        &pu,
    )
    .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;

    let pm: [&dyn ToSqlTurso; 4] = [&ws_id, &user_id, &effective_role, &now];
    db.execute(
        "INSERT INTO workspace_members(workspace_id,user_id,role,added_at) VALUES(?,?,?,?) \
         ON CONFLICT(workspace_id,user_id) DO UPDATE SET role=excluded.role",
        &pm,
    )
    .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;

    // 既定 config は新しい会社のときだけ。既存 workspace に入れると、
    // その会社が登録済みのキャンペーンを空で上書きしてしまう。
    if !joining {
        let key = CONFIG_KEY.to_string();
        let cfg = DEFAULT_CONFIG_JSON.to_string();
        let pc: [&dyn ToSqlTurso; 3] = [&ws_id, &key, &cfg];
        let _ = db.execute(
            "INSERT OR REPLACE INTO kv_settings(workspace_id,key,value) VALUES(?,?,?)",
            &pc,
        );
    }

    Ok(json!({
        "ok": true,
        "company": company,
        "email": email,
        "user_id": user_id,
        "workspace_id": ws_id,
        "role": effective_role,
        "joined_existing": joining,
    }))
}

// ==== master 用ユーザー管理API（認証は master のログインセッション。SCOUT_ADMIN_TOKEN 不要） ====
// 末端顧客の EXE には管理トークンを一切入れない設計。master がUIからログインして操作する。

/// master 用: 全ユーザー一覧（会社=workspace名, role, 無効状態）。
async fn admin_list_users(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        require_master(&dbh, &token)?;
        ensure_user_disabled_column(&dbh);
        ensure_user_role_column(&dbh);
        let rows = dbh
            .query(
                "SELECT u.id,u.email,u.name,COALESCE(u.role,'member') AS role,\
                 COALESCE(u.disabled,0) AS disabled,u.created_at,\
                 (SELECT w.name FROM workspaces w WHERE w.owner_user_id=u.id LIMIT 1) AS company \
                 FROM users u ORDER BY u.created_at",
                &[],
            )
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let users: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({
                    "id": get_str(r, "id"),
                    "email": get_str(r, "email"),
                    "name": get_str(r, "name"),
                    "role": get_str(r, "role"),
                    "company": get_str(r, "company"),
                    "disabled": r.get("disabled").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
                    "created_at": get_str(r, "created_at"),
                })
            })
            .collect();
        Ok(json!({ "ok": true, "users": users }))
    })
    .await
}

/// master 用: ユーザー作成（provision と同じ効果。認証は master セッション）。
async fn admin_create_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let company = body
        .get("company")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name_in = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let name = if name_in.is_empty() {
        company.clone()
    } else {
        name_in
    };
    let role = match body
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("member")
        .trim()
    {
        "master" => "master".to_string(),
        _ => "member".to_string(),
    };
    // 既存の会社に担当者を足す場合だけ指定する。空なら新しい会社として作る。
    let workspace_id = body
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        // master が自分の workspace に担当者を足す場合は、その workspace へ入れる。
        // 指定が無ければ従来どおり新しい会社として作る。
        let me = require_master(&dbh, &token)?;
        let ws = if workspace_id.is_empty() {
            String::new()
        } else if workspace_id == me.workspace_id {
            workspace_id.clone()
        } else {
            // 他社の workspace へ勝手に人を入れられないようにする。
            return Err(cerr(
                StatusCode::FORBIDDEN,
                "自分の workspace 以外には追加できません",
            ));
        };
        provision_core(&dbh, company, email, password, name, role, ws)
    })
    .await
}

/// master 用: 任意ユーザーのパスワード再設定。該当ユーザーの全セッションを失効（再ログイン強制）。
async fn admin_reset_password(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let new_password = body
        .get("new_password")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        require_master(&dbh, &token)?;
        if email.is_empty() || new_password.len() < 8 {
            return Err(cerr(
                StatusCode::BAD_REQUEST,
                "email・8文字以上の new_password が必要です",
            ));
        }
        let pe: [&dyn ToSqlTurso; 1] = [&email];
        let urows = dbh
            .query("SELECT id FROM users WHERE email=?", &pe)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let urow = urows
            .first()
            .ok_or_else(|| cerr(StatusCode::NOT_FOUND, "該当ユーザーがいません"))?;
        let uid = get_str(urow, "id");
        let hash = bcrypt::hash(&new_password, bcrypt::DEFAULT_COST)
            .map_err(|_| cerr(StatusCode::INTERNAL_SERVER_ERROR, "ハッシュ生成失敗"))?;
        let pu: [&dyn ToSqlTurso; 2] = [&hash, &email];
        dbh.execute("UPDATE users SET password_hash=? WHERE email=?", &pu)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        // 新パスワードで再ログインを強制（旧セッションを無効化）。
        let ps: [&dyn ToSqlTurso; 1] = [&uid];
        let _ = dbh.execute("DELETE FROM auth_sessions WHERE user_id=?", &ps);
        Ok(json!({ "ok": true, "email": email }))
    })
    .await
}

#[cfg(test)]
mod config_patch_tests {
    use super::CONFIG_PATCH_SQL;
    use rusqlite::{params, Connection};
    use serde_json::{json, Value};

    #[test]
    fn section_patch_preserves_prompt_when_campaigns_are_saved() {
        let db = Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE kv_settings(\
             workspace_id TEXT NOT NULL,key TEXT NOT NULL,value TEXT NOT NULL,\
             PRIMARY KEY(workspace_id,key))",
            [],
        )
        .unwrap();

        let current = json!({
            "campaigns": [{"name": "old"}],
            "prompt_templates": [{"id": "p1", "platform": "openwork", "text": "saved prompt"}],
            "resend_templates": [{"id": "r1", "body": "saved resend"}]
        })
        .to_string();
        db.execute(
            "INSERT INTO kv_settings(workspace_id,key,value) VALUES(?,?,?)",
            params!["ws1", "__config__", current],
        )
        .unwrap();

        let campaign_patch = json!({"campaigns": [{"name": "campaign_1"}]}).to_string();
        db.execute(
            CONFIG_PATCH_SQL,
            params!["ws1", "__config__", "{}", campaign_patch],
        )
        .unwrap();

        let stored: String = db
            .query_row(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                params!["ws1", "__config__"],
                |row| row.get(0),
            )
            .unwrap();
        let config: Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(config["campaigns"][0]["name"], "campaign_1");
        assert_eq!(config["prompt_templates"][0]["text"], "saved prompt");
        assert_eq!(config["resend_templates"][0]["body"], "saved resend");
    }

    #[test]
    fn section_patch_preserves_campaigns_when_prompt_is_saved() {
        let db = Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE kv_settings(\
             workspace_id TEXT NOT NULL,key TEXT NOT NULL,value TEXT NOT NULL,\
             PRIMARY KEY(workspace_id,key))",
            [],
        )
        .unwrap();
        let current = json!({"campaigns": [{"name": "campaign_1"}]}).to_string();
        db.execute(
            "INSERT INTO kv_settings(workspace_id,key,value) VALUES(?,?,?)",
            params!["ws1", "__config__", current],
        )
        .unwrap();

        let prompt_patch = json!({
            "prompt_templates": [{"id": "p1", "platform": "openwork", "text": "saved prompt"}]
        })
        .to_string();
        db.execute(
            CONFIG_PATCH_SQL,
            params!["ws1", "__config__", "{}", prompt_patch],
        )
        .unwrap();

        let stored: String = db
            .query_row(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                params!["ws1", "__config__"],
                |row| row.get(0),
            )
            .unwrap();
        let config: Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(config["campaigns"][0]["name"], "campaign_1");
        assert_eq!(config["prompt_templates"][0]["text"], "saved prompt");
    }
}

#[cfg(test)]
mod run_log_tests {
    //! 実行ログの SQL を**実装の定数そのまま**ローカル SQLite に流して確かめる。
    //! 「列が存在する」ではなく、入った値・絞り込み結果・列の順序を具体値で見る。
    use super::{
        build_run_log_csv, clip_chars, run_log_csv_row, RUN_LOG_CAMPAIGNS_EXISTS_SQL,
        RUN_LOG_CSV_COLUMNS, RUN_LOG_CSV_HEADER, RUN_LOG_ERROR_COLUMN_ALTER_SQL,
        RUN_LOG_EXPORT_MAX_ROWS, RUN_LOG_EXPORT_SQL, RUN_LOG_EXPORT_SQL_WITHOUT_CAMPAIGNS,
        RUN_LOG_INSERT_SQL, RUN_LOG_PLATFORM_ALTER_SQL, RUN_LOG_TRUNCATED_NOTICE,
    };
    use rusqlite::types::ValueRef;
    use rusqlite::{params, Connection};
    use serde_json::Value;
    use std::collections::HashMap;

    /// 中央DBに実在する run_logs の形 + 起動時に後付けする5列を、実装と同じ ALTER 文で作る。
    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE run_logs(id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT, \
             campaign_id TEXT, iter INTEGER, candidate_web_id TEXT, subject_chars INTEGER, \
             body_chars INTEGER, api_seconds REAL, status TEXT, \
             workspace_id TEXT NOT NULL DEFAULT 'default', user_id TEXT)",
            [],
        )
        .unwrap();
        db.execute(RUN_LOG_PLATFORM_ALTER_SQL, []).unwrap();
        for sql in RUN_LOG_ERROR_COLUMN_ALTER_SQL {
            db.execute(sql, []).unwrap();
        }
        db
    }

    fn with_campaigns(db: &Connection) {
        db.execute(
            "CREATE TABLE campaigns(id TEXT, workspace_id TEXT, name TEXT, platform TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO campaigns(id,workspace_id,name,platform) \
             VALUES('c1','ws1','新卒_AMBI','ambi'),('c1','ws2','他社の同IDキャンペーン','green')",
            [],
        )
        .unwrap();
    }

    /// `POST /scout/api/run-log` が流すのと同じ 15 パラメータで1行入れる。
    fn insert(
        db: &Connection,
        ts: &str,
        campaign: &str,
        candidate: &str,
        ws: &str,
        platform: &str,
        status: &str,
    ) {
        db.execute(
            RUN_LOG_INSERT_SQL,
            params![
                ts,
                campaign,
                12_i64,
                candidate,
                48_i64,
                820_i64,
                5.2_f64,
                status,
                ws,
                "user-1",
                platform,
                "send",
                status,
                "件名は50文字以内で入力してください",
                "https://example.test/scout"
            ],
        )
        .unwrap();
    }

    /// Turso の行表現(`HashMap<String, Value>`)に合わせて読み出す。CSV 生成関数へそのまま渡せる。
    fn export(
        db: &Connection,
        sql: &str,
        ws: &str,
        from: &str,
        to: &str,
        platform: &str,
    ) -> Vec<HashMap<String, Value>> {
        let mut stmt = db.prepare(sql).unwrap();
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let mut q = stmt
            .query(params![ws, from, from, to, to, platform, platform])
            .unwrap();
        while let Some(row) = q.next().unwrap() {
            let mut map = HashMap::new();
            for (i, name) in names.iter().enumerate() {
                let v = match row.get_ref(i).unwrap() {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => Value::from(n),
                    ValueRef::Real(f) => serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or(Value::Null),
                    ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).to_string()),
                    ValueRef::Blob(_) => Value::Null,
                };
                map.insert(name.clone(), v);
            }
            out.push(map);
        }
        out
    }

    fn cell(rows: &[HashMap<String, Value>], i: usize, key: &str) -> String {
        match rows[i].get(key) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    }

    /// ログなので冪等化しない。同じ候補者が2回弾かれたら2行残る。
    #[test]
    fn run_log_insert_keeps_every_row_for_the_same_candidate() {
        let db = database();
        insert(&db, "2026-10-08T10:00:00", "c1", "w-1", "ws1", "ambi", "verify_failed");
        insert(&db, "2026-10-08T10:05:00", "c1", "w-1", "ws1", "ambi", "skipped_sent");
        let n: i64 = db
            .query_row("SELECT COUNT(*) FROM run_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        let (status, kind, detail, platform, secs, iter): (String, String, String, String, f64, i64) =
            db.query_row(
                "SELECT status,error_kind,error_detail,platform,api_seconds,iter FROM run_logs \
                 ORDER BY id LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(status, "verify_failed");
        assert_eq!(kind, "send");
        assert_eq!(detail, "件名は50文字以内で入力してください");
        assert_eq!(platform, "ambi");
        assert!((secs - 5.2).abs() < 1e-9);
        assert_eq!(iter, 12);
    }

    /// 他社(別 workspace)の行は1行も混ざらない。campaign_name は同じ workspace の名前だけ引く。
    #[test]
    fn export_scopes_rows_to_the_callers_workspace() {
        let db = database();
        with_campaigns(&db);
        insert(&db, "2026-10-08T09:00:00", "c1", "w-1", "ws1", "ambi", "sent");
        insert(&db, "2026-10-07T09:00:00", "c9", "w-2", "ws1", "green", "skipped_sent");
        insert(&db, "2026-10-08T09:30:00", "c1", "w-3", "ws2", "ambi", "sent");
        let rows = export(&db, RUN_LOG_EXPORT_SQL, "ws1", "", "", "");
        assert_eq!(rows.len(), 2);
        // ts 昇順。
        assert_eq!(cell(&rows, 0, "ts"), "2026-10-07T09:00:00");
        assert_eq!(cell(&rows, 1, "ts"), "2026-10-08T09:00:00");
        // 未知のキャンペーンIDは空、既知は自分の workspace の名前。
        assert_eq!(cell(&rows, 0, "campaign_name"), "");
        assert_eq!(cell(&rows, 1, "campaign_name"), "新卒_AMBI");
        assert_eq!(cell(&rows, 1, "candidate_web_id"), "w-1");
    }

    #[test]
    fn export_filters_by_from_to_and_platform() {
        let db = database();
        with_campaigns(&db);
        insert(&db, "2026-10-05T09:00:00", "c1", "w-1", "ws1", "ambi", "sent");
        insert(&db, "2026-10-07T09:00:00", "c1", "w-2", "ws1", "ambi", "verify_failed");
        insert(&db, "2026-10-09T09:00:00", "c1", "w-3", "ws1", "green", "sent");
        // from のみ
        let r = export(&db, RUN_LOG_EXPORT_SQL, "ws1", "2026-10-07", "", "");
        assert_eq!(r.len(), 2);
        // from..to（両端を含む）
        let r = export(&db, RUN_LOG_EXPORT_SQL, "ws1", "2026-10-07", "2026-10-07", "");
        assert_eq!(r.len(), 1);
        assert_eq!(cell(&r, 0, "candidate_web_id"), "w-2");
        // platform のみ
        let r = export(&db, RUN_LOG_EXPORT_SQL, "ws1", "", "", "green");
        assert_eq!(r.len(), 1);
        assert_eq!(cell(&r, 0, "candidate_web_id"), "w-3");
        // 全部空は無指定（3件）
        assert_eq!(export(&db, RUN_LOG_EXPORT_SQL, "ws1", "", "", "").len(), 3);
    }

    /// `campaigns` 表が無い DB でも落ちず、列の名前と順序は join 版と同じまま。
    #[test]
    fn export_works_without_a_campaigns_table() {
        let db = database();
        insert(&db, "2026-10-08T09:00:00", "c1", "w-1", "ws1", "ambi", "sent");
        assert!(db
            .prepare(RUN_LOG_CAMPAIGNS_EXISTS_SQL)
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .count()
            .eq(&0));
        let rows = export(&db, RUN_LOG_EXPORT_SQL_WITHOUT_CAMPAIGNS, "ws1", "", "", "");
        assert_eq!(rows.len(), 1);
        assert_eq!(cell(&rows, 0, "campaign_name"), "");
        assert_eq!(cell(&rows, 0, "status"), "sent");

        let db2 = database();
        with_campaigns(&db2);
        assert_eq!(
            db2.prepare(RUN_LOG_CAMPAIGNS_EXISTS_SQL)
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .count(),
            1
        );
    }

    /// SQL が返す列の名前と順序が CSV の見出しと1対1であること。
    /// ここがズレると、見出しと中身が違う CSV を黙って配ることになる。
    #[test]
    fn export_sql_columns_match_the_csv_header() {
        let db = database();
        with_campaigns(&db);
        let expected: Vec<&str> = RUN_LOG_CSV_HEADER.split(',').collect();
        assert_eq!(expected.len(), RUN_LOG_CSV_COLUMNS);
        for sql in [RUN_LOG_EXPORT_SQL, RUN_LOG_EXPORT_SQL_WITHOUT_CAMPAIGNS] {
            let stmt = db.prepare(sql).unwrap();
            assert_eq!(stmt.column_names(), expected);
        }
        // 上限+1件取って打ち切りを検知する約束。
        assert!(RUN_LOG_EXPORT_SQL.ends_with(&format!("LIMIT {}", RUN_LOG_EXPORT_MAX_ROWS + 1)));
        assert!(RUN_LOG_EXPORT_SQL_WITHOUT_CAMPAIGNS
            .ends_with(&format!("LIMIT {}", RUN_LOG_EXPORT_MAX_ROWS + 1)));
    }

    /// `error_detail` には画面の文言が入る。カンマ・引用符・改行で列がずれないこと。
    #[test]
    fn csv_quotes_commas_quotes_and_newlines() {
        let mut row: HashMap<String, Value> = HashMap::new();
        row.insert("ts".into(), Value::from("2026-10-08T10:00:00"));
        row.insert("campaign_id".into(), Value::from("c1"));
        row.insert("campaign_name".into(), Value::from("新卒, AMBI"));
        row.insert("platform".into(), Value::from("ambi"));
        row.insert("candidate_web_id".into(), Value::from("w-1"));
        row.insert("status".into(), Value::from("verify_failed"));
        row.insert("subject_chars".into(), Value::from(48));
        row.insert("body_chars".into(), Value::from(820));
        row.insert("api_seconds".into(), Value::from(5.2));
        row.insert("error_kind".into(), Value::from("send"));
        row.insert("error_step".into(), Value::from("verify_failed"));
        row.insert(
            "error_detail".into(),
            Value::from("件名は\"50文字\"以内,\n入力してください"),
        );
        row.insert("error_url".into(), Value::from("https://example.test/x?a=1"));
        row.insert("user_id".into(), Value::from("user-1"));
        let line = run_log_csv_row(&row);
        assert!(line.starts_with("2026-10-08T10:00:00,c1,\"新卒, AMBI\",ambi,w-1,verify_failed,48,820,5.2,send,verify_failed,"));
        assert!(line.contains("\"件名は\"\"50文字\"\"以内,\n入力してください\""));
        assert!(line.ends_with(",https://example.test/x?a=1,user-1"));

        // 値が無い数値列は 0 ではなく空欄（「0文字」と「記録が無い」を取り違えない）。
        let empty: HashMap<String, Value> = HashMap::new();
        assert_eq!(run_log_csv_row(&empty), ",".repeat(RUN_LOG_CSV_COLUMNS - 1));
    }

    #[test]
    fn csv_has_bom_and_shows_truncation_in_the_body() {
        let rows: Vec<HashMap<String, Value>> = vec![HashMap::new(); 3];
        let (csv, truncated) = build_run_log_csv(&rows);
        assert!(!truncated);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv[3..].starts_with(RUN_LOG_CSV_HEADER));
        assert_eq!(csv.matches("\r\n").count(), 4); // 見出し + 3行
        assert!(!csv.contains(RUN_LOG_TRUNCATED_NOTICE));

        let many: Vec<HashMap<String, Value>> = vec![HashMap::new(); RUN_LOG_EXPORT_MAX_ROWS + 1];
        let (csv, truncated) = build_run_log_csv(&many);
        assert!(truncated);
        // 出すのは上限ぶんだけ。最終行は打ち切りの注記（1列目）。
        assert_eq!(csv.matches("\r\n").count(), RUN_LOG_EXPORT_MAX_ROWS + 2);
        let last = csv.trim_end_matches("\r\n").lines().last().unwrap().to_string();
        assert_eq!(
            last,
            format!(
                "{}{}",
                RUN_LOG_TRUNCATED_NOTICE,
                ",".repeat(RUN_LOG_CSV_COLUMNS - 1)
            )
        );
    }

    #[test]
    fn clip_chars_counts_characters_not_bytes() {
        let s = "あ".repeat(600);
        assert_eq!(clip_chars(&s, 500).chars().count(), 500);
        assert_eq!(clip_chars("abc", 500), "abc");
    }
}

#[cfg(test)]
mod run_log_route_tests {
    //! ルートが本当に生えているかを HTTP の層で見る。
    //! `router()` に書き足しただけでは、メソッド違い(405)や綴り違い(404)に気づけない。
    //! scout DB 未設定なら 503 が返るのが正しい。**404/405 でないこと**で配線を確かめる。
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::auth::session::RateLimiter;
    use crate::config::AppConfig;
    use crate::db::cache::AppCache;
    use crate::AppState;

    fn bare_state() -> Arc<AppState> {
        let config = AppConfig::from_env();
        let cache = AppCache::new(config.cache_ttl_secs, config.cache_max_entries);
        let rate_limiter = RateLimiter::new(
            config.rate_limit_max_attempts,
            config.rate_limit_lockout_secs,
        );
        Arc::new(AppState {
            config,
            hw_db: None,
            indeed_db: None,
            turso_db: None,
            salesnow_db: None,
            scout_db: None,
            cache,
            rate_limiter,
            company_geo_cache: None,
            audit: None,
        })
    }

    #[tokio::test]
    async fn run_log_routes_are_wired() {
        let app = super::router().with_state(bare_state());
        let post = Request::builder()
            .method("POST")
            .uri("/scout/api/run-log")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"campaign_id":"c1","status":"verify_failed"}"#))
            .unwrap();
        let res = app.clone().oneshot(post).await.unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);

        let get = Request::builder()
            .method("GET")
            .uri("/scout/api/run-log/export?from=2026-10-01&to=2026-10-08&platform=ambi")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(get).await.unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}

#[cfg(test)]
mod credential_tests {
    use super::{
        credential_response_headers, credential_status_value, credential_storage_key,
        credentials_key_from_encoded, decode_credentials_key, decrypt_credentials,
        encrypt_credentials, platform_credentials_value, require_credentials_token,
        require_valid_platform_slug, valid_platform_slug, StoredCredentials, CREDENTIAL_UPSERT_SQL,
    };
    use axum::http::StatusCode;
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
    use rusqlite::{params, Connection};

    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE kv_settings(\
             workspace_id TEXT NOT NULL,key TEXT NOT NULL,value TEXT NOT NULL,\
             PRIMARY KEY(workspace_id,key))",
            [],
        )
        .unwrap();
        db
    }

    fn encrypted(key: &[u8; 32], workspace: &str, platform: &str, user: &str) -> String {
        encrypt_credentials(
            key,
            workspace,
            platform,
            &StoredCredentials {
                username: user.to_string(),
                password: format!("secret-{user}"),
            },
        )
        .unwrap()
    }

    #[test]
    fn credentials_encrypt_and_decrypt_with_matching_aad() {
        let key = [7_u8; 32];
        let stored = encrypted(&key, "workspace-a", "openwork", "alice");
        let result = decrypt_credentials(&key, "workspace-a", "openwork", &stored).unwrap();
        assert_eq!(result.username, "alice");
        assert_eq!(result.password, "secret-alice");
        assert!(!stored.contains("alice"));
        assert!(!stored.contains("secret-alice"));
    }

    #[test]
    fn credentials_reject_wrong_key_or_aad() {
        let key = [7_u8; 32];
        let stored = encrypted(&key, "workspace-a", "openwork", "alice");
        assert!(decrypt_credentials(&[8_u8; 32], "workspace-a", "openwork", &stored).is_err());
        assert!(decrypt_credentials(&key, "workspace-b", "openwork", &stored).is_err());
        assert!(decrypt_credentials(&key, "workspace-a", "green", &stored).is_err());
    }

    #[test]
    fn credentials_key_requires_base64_encoded_32_bytes() {
        let encoded = BASE64_STANDARD.encode([9_u8; 32]);
        assert_eq!(decode_credentials_key(&encoded).unwrap(), [9_u8; 32]);
        assert!(decode_credentials_key("not-base64").is_err());
        assert!(decode_credentials_key(&BASE64_STANDARD.encode([9_u8; 31])).is_err());
        assert_eq!(
            credentials_key_from_encoded(None).unwrap_err().0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            credentials_key_from_encoded(Some("invalid")).unwrap_err().0,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn platform_slug_accepts_future_media_without_allowlist_changes() {
        for valid in ["openwork", "green", "ambi", "mynavi-2027", "offerbox"] {
            assert!(valid_platform_slug(valid), "{valid}");
        }
        for invalid in [
            "",
            "OpenWork",
            "open_work",
            "-openwork",
            "open/work",
            "日本語",
        ] {
            assert!(!valid_platform_slug(invalid), "{invalid}");
            assert_eq!(
                require_valid_platform_slug(invalid).unwrap_err().0,
                StatusCode::BAD_REQUEST
            );
        }
    }

    #[test]
    fn media_upsert_preserves_other_media() {
        let db = database();
        let key = [1_u8; 32];
        let workspace = "workspace-a";
        let openwork_key = credential_storage_key("openwork");
        let green_key = credential_storage_key("green");
        db.execute(
            CREDENTIAL_UPSERT_SQL,
            params![
                workspace,
                openwork_key,
                encrypted(&key, workspace, "openwork", "old")
            ],
        )
        .unwrap();
        db.execute(
            CREDENTIAL_UPSERT_SQL,
            params![
                workspace,
                green_key,
                encrypted(&key, workspace, "green", "green-user")
            ],
        )
        .unwrap();
        db.execute(
            CREDENTIAL_UPSERT_SQL,
            params![
                workspace,
                openwork_key,
                encrypted(&key, workspace, "openwork", "new")
            ],
        )
        .unwrap();

        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM kv_settings WHERE workspace_id=?",
                params![workspace],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);

        let stored_openwork: String = db
            .query_row(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                params![workspace, openwork_key],
                |row| row.get(0),
            )
            .unwrap();
        let stored_green: String = db
            .query_row(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                params![workspace, green_key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            decrypt_credentials(&key, workspace, "openwork", &stored_openwork)
                .unwrap()
                .username,
            "new"
        );
        assert_eq!(
            decrypt_credentials(&key, workspace, "green", &stored_green)
                .unwrap()
                .username,
            "green-user"
        );
    }

    #[test]
    fn credentials_are_isolated_by_workspace() {
        let db = database();
        let key = [2_u8; 32];
        let storage_key = credential_storage_key("openwork");
        for (workspace, user) in [("workspace-a", "alice"), ("workspace-b", "bob")] {
            db.execute(
                CREDENTIAL_UPSERT_SQL,
                params![
                    workspace,
                    storage_key,
                    encrypted(&key, workspace, "openwork", user)
                ],
            )
            .unwrap();
        }
        let stored_a: String = db
            .query_row(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                params!["workspace-a", storage_key],
                |row| row.get(0),
            )
            .unwrap();
        let stored_b: String = db
            .query_row(
                "SELECT value FROM kv_settings WHERE workspace_id=? AND key=?",
                params!["workspace-b", storage_key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            decrypt_credentials(&key, "workspace-a", "openwork", &stored_a)
                .unwrap()
                .username,
            "alice"
        );
        assert_eq!(
            decrypt_credentials(&key, "workspace-b", "openwork", &stored_b)
                .unwrap()
                .username,
            "bob"
        );
        assert!(decrypt_credentials(&key, "workspace-b", "openwork", &stored_a).is_err());
    }

    #[test]
    fn status_never_contains_password_or_ciphertext() {
        let credentials = StoredCredentials {
            username: "alice@example.com".to_string(),
            password: "top-secret".to_string(),
        };
        let status = credential_status_value(&credentials);
        let serialized = serde_json::to_string(&status).unwrap();
        assert_eq!(status["username"], "alice@example.com");
        assert_eq!(status["password_set"], true);
        assert!(status.get("password").is_none());
        assert!(!serialized.contains("top-secret"));
        assert!(!serialized.contains("ciphertext"));
    }

    #[test]
    fn full_credentials_contract_is_wrapped() {
        let credentials = StoredCredentials {
            username: "alice@example.com".to_string(),
            password: "top-secret".to_string(),
        };
        let value = platform_credentials_value("openwork", &credentials);
        assert_eq!(value["credential"]["platform"], "openwork");
        assert_eq!(value["credential"]["username"], "alice@example.com");
        assert_eq!(value["credential"]["password"], "top-secret");
        assert!(value.get("password").is_none());
    }

    #[test]
    fn credentials_responses_disable_caching() {
        let headers = credential_response_headers();
        assert_eq!(headers["cache-control"], "no-store, private");
    }

    #[test]
    fn missing_auth_token_is_unauthorized() {
        let error = require_credentials_token("").unwrap_err();
        assert_eq!(error.0, StatusCode::UNAUTHORIZED);
        assert!(require_credentials_token("session-token").is_ok());
    }
}

// ==== 担当者(workspace メンバー)管理 ====
// 1社に複数の担当者を置くための API。実運用では複数PCで新卒媒体と中途媒体を
// 分けて同時に回しており、誰の送信かを残すために担当者ごとのアカウントが要る。
// 認証は master のログインセッション(末端顧客の配布物に管理トークンは入れない)。

/// 自分の workspace の担当者一覧。所有者も含める。
///
/// 所有者も拾うのは互換のため: `workspace_members` は後から足した表なので、
/// 既存顧客は所有者行しか持たない。members だけを見ると**既存顧客が
/// 「担当者0人」に見える**。
fn list_members_core(db: &TursoDb, workspace_id: &str) -> Result<Value, CoreErr> {
    ensure_workspace_members_table(db);
    ensure_user_disabled_column(db);
    ensure_user_role_column(db);
    ensure_user_last_login_column(db);
    let wid = workspace_id.to_string();
    let p: [&dyn ToSqlTurso; 2] = [&wid, &wid];
    let rows = db
        .query(
            "SELECT u.id,u.email,u.name,u.last_login,\
             COALESCE(u.disabled,0) AS disabled,\
             COALESCE(m.role, CASE WHEN w.owner_user_id=u.id THEN 'master' END, 'member') AS role,\
             m.added_at,\
             CASE WHEN w.owner_user_id=u.id THEN 1 ELSE 0 END AS is_owner \
             FROM users u \
             LEFT JOIN workspace_members m ON m.user_id=u.id AND m.workspace_id=? \
             LEFT JOIN workspaces w ON w.id=? \
             WHERE m.user_id IS NOT NULL OR w.owner_user_id=u.id \
             ORDER BY is_owner DESC, u.email",
            &p,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let members: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "user_id": get_str(r, "id"),
                "email": get_str(r, "email"),
                "name": get_str(r, "name"),
                "role": get_str(r, "role"),
                "disabled": r.get("disabled").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
                "last_login": get_str(r, "last_login"),
                "added_at": get_str(r, "added_at"),
                "is_owner": r.get("is_owner").and_then(|v| v.as_i64()).unwrap_or(0) != 0,
            })
        })
        .collect();
    Ok(json!({ "ok": true, "workspace_id": workspace_id, "members": members }))
}

async fn admin_list_members(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let token = token_from(&headers);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let me = require_master(&dbh, &token)?;
        list_members_core(&dbh, &me.workspace_id)
    })
    .await
}

/// 既存ユーザーを自分の workspace の担当者に加える。
///
/// ここでユーザーを新規作成しないのは、作成が provision / admin_create_user に
/// 閉じているから。作成経路を増やすと、パスワードのハッシュ方式がまた食い違う。
async fn admin_add_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let role = match body
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("member")
        .trim()
    {
        "master" => "master".to_string(),
        _ => "member".to_string(),
    };
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let me = require_master(&dbh, &token)?;
        if email.is_empty() {
            return Err(cerr(StatusCode::BAD_REQUEST, "email が必要です"));
        }
        ensure_workspace_members_table(&dbh);
        let pe: [&dyn ToSqlTurso; 1] = [&email];
        let rows = dbh
            .query("SELECT id FROM users WHERE email=?", &pe)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let uid = rows
            .first()
            .map(|r| get_str(r, "id"))
            .ok_or_else(|| cerr(StatusCode::NOT_FOUND, "そのメールのアカウントがありません"))?;
        let now = now_str();
        let pm: [&dyn ToSqlTurso; 4] = [&me.workspace_id, &uid, &role, &now];
        dbh.execute(
            "INSERT INTO workspace_members(workspace_id,user_id,role,added_at) VALUES(?,?,?,?) \
             ON CONFLICT(workspace_id,user_id) DO UPDATE SET role=excluded.role",
            &pm,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        list_members_core(&dbh, &me.workspace_id)
    })
    .await
}

/// 担当者を外す。所有者は外せない。
///
/// 所有者を外せると、設定も履歴も残ったまま**誰も入れない workspace** ができる。
async fn admin_remove_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let me = require_master(&dbh, &token)?;
        if target.is_empty() {
            return Err(cerr(StatusCode::BAD_REQUEST, "user_id が必要です"));
        }
        ensure_workspace_members_table(&dbh);
        let pw: [&dyn ToSqlTurso; 1] = [&me.workspace_id];
        let wrows = dbh
            .query("SELECT owner_user_id FROM workspaces WHERE id=?", &pw)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        if wrows
            .first()
            .map(|r| get_str(r, "owner_user_id"))
            .unwrap_or_default()
            == target
        {
            return Err(cerr(
                StatusCode::BAD_REQUEST,
                "所有者は担当者から外せません",
            ));
        }
        let pd: [&dyn ToSqlTurso; 2] = [&me.workspace_id, &target];
        dbh.execute(
            "DELETE FROM workspace_members WHERE workspace_id=? AND user_id=?",
            &pd,
        )
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        // 外した人のセッションを失効させる(次の要求から入れなくなる)。
        let ps: [&dyn ToSqlTurso; 1] = [&target];
        let _ = dbh.execute("DELETE FROM auth_sessions WHERE user_id=?", &ps);
        list_members_core(&dbh, &me.workspace_id)
    })
    .await
}

/// 担当者の利用を停止/再開する。
///
/// **削除ではない。** 削除すると send_history.user_id が宛先を失い、過去の送信が
/// 誰のものだったか辿れなくなる。退職者は停止して残す。
async fn admin_set_member_disabled(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let token = token_from(&headers);
    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let disabled = body
        .get("disabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        let me = require_master(&dbh, &token)?;
        if target.is_empty() {
            return Err(cerr(StatusCode::BAD_REQUEST, "user_id が必要です"));
        }
        if target == me.user_id && disabled {
            // 自分を止めると、他に master が居なければ誰も担当者管理できなくなる。
            return Err(cerr(
                StatusCode::BAD_REQUEST,
                "自分自身を停止することはできません",
            ));
        }
        ensure_workspace_members_table(&dbh);
        ensure_user_disabled_column(&dbh);
        // 自分の workspace の担当者だけを対象にする(他社のユーザーを止めさせない)。
        // 所有者も対象に含める: members 行を持たない既存顧客の所有者を取りこぼさないため。
        let pb: [&dyn ToSqlTurso; 4] = [&me.workspace_id, &target, &me.workspace_id, &target];
        let belongs = dbh
            .query(
                "SELECT 1 AS x FROM workspace_members WHERE workspace_id=? AND user_id=? \
                 UNION SELECT 1 AS x FROM workspaces WHERE id=? AND owner_user_id=?",
                &pb,
            )
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        if belongs.is_empty() {
            return Err(cerr(
                StatusCode::NOT_FOUND,
                "その担当者はこの workspace にいません",
            ));
        }
        let flag: i64 = if disabled { 1 } else { 0 };
        let pu: [&dyn ToSqlTurso; 2] = [&flag, &target];
        dbh.execute("UPDATE users SET disabled=? WHERE id=?", &pu)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        if disabled {
            let ps: [&dyn ToSqlTurso; 1] = [&target];
            let _ = dbh.execute("DELETE FROM auth_sessions WHERE user_id=?", &ps);
        }
        list_members_core(&dbh, &me.workspace_id)
    })
    .await
}
