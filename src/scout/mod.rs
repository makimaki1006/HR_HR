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

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::db::turso_http::{ToSqlTurso, TursoDb};
use crate::AppState;

const SESSION_TTL_DAYS: i64 = 7;
const CONFIG_KEY: &str = "__config__";
const STATE_KEY: &str = "__state__";
const CONFIG_PATCH_SQL: &str =
    "INSERT INTO kv_settings(workspace_id,key,value) VALUES(?,?,?) \
     ON CONFLICT(workspace_id,key) DO UPDATE SET \
     value=json_patch(COALESCE(kv_settings.value,'{}'), ?)";

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
        .route("/scout/api/state", get(get_state).post(save_state))
        .route("/scout/api/sent", post(sent))
        .route("/scout/api/has-sent", get(has_sent))
        .route("/scout/api/killswitch", get(killswitch))
        .route("/scout/api/admin/killswitch", post(admin_killswitch))
        .route("/scout/api/admin/disable", post(admin_disable))
        .route("/scout/api/admin/provision", post(provision))
        .route("/scout/api/admin/users", get(admin_list_users).post(admin_create_user))
        .route("/scout/api/admin/reset-password", post(admin_reset_password))
}

// ===== 型・共通ヘルパー =====

/// コア関数(同期)のエラー: (HTTPステータス, メッセージ)
type CoreErr = (StatusCode, String);
type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

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

fn now_str() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn get_str(row: &HashMap<String, Value>, key: &str) -> String {
    row.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
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
    let t = token.to_string();
    let params: [&dyn ToSqlTurso; 1] = [&t];
    // role はユーザーの最新値を users から解決（session 発行後の role 変更も即反映）。
    let rows = db
        .query(
            "SELECT s.user_id,s.email,s.name,s.workspace_id,s.expires_at,\
             COALESCE(u.role,'member') AS role \
             FROM auth_sessions s LEFT JOIN users u ON u.id=s.user_id WHERE s.token=?",
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
    current_user(db, token)
        .ok_or_else(|| cerr(StatusCode::UNAUTHORIZED, "未ログイン(トークンが無効です)"))
}

/// users テーブルへ `disabled` 列を後付けする(解約遮断用)。プロセス生存中に一度だけ実行。
/// 既に列がある/ALTER 非対応でも失敗を無視する(冪等)。※同期。spawn_blocking 内で呼ぶこと。
fn ensure_user_disabled_column(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        // 既に列が存在すると Turso はエラーを返すが、それは正常系として無視する。
        let _ = db.execute("ALTER TABLE users ADD COLUMN disabled INTEGER DEFAULT 0", &[]);
    });
}

/// users テーブルへ `role` 列を後付けする(master/member の権限分離用)。プロセス生存中に一度だけ。
/// 既定は 'member'。既存ユーザーは全員 member 扱いになる(master は明示昇格が必要)。※同期。
fn ensure_user_role_column(db: &TursoDb) {
    static ENSURED: OnceLock<()> = OnceLock::new();
    ENSURED.get_or_init(|| {
        let _ = db.execute("ALTER TABLE users ADD COLUMN role TEXT DEFAULT 'member'", &[]);
    });
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
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("").trim().to_lowercase();
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || login_core(&dbh, email, password)).await
}

fn login_core(db: &TursoDb, email: String, password: String) -> Result<Value, CoreErr> {
    if email.is_empty() || password.is_empty() {
        return Err(cerr(StatusCode::BAD_REQUEST, "メールとパスワードが必要です"));
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
        return Err(cerr(StatusCode::UNAUTHORIZED, "メールまたはパスワードが違います"));
    }
    // パスワード検証成功後に無効化チェック(解約済みアカウントを遮断)。
    let disabled = row.get("disabled").and_then(|v| v.as_i64()).unwrap_or(0);
    if disabled != 0 {
        return Err(cerr(StatusCode::FORBIDDEN, "アカウントが無効化されています"));
    }
    let user_id = get_str(row, "id");
    let name = get_str(row, "name");
    let role = get_str(row, "role");

    let p2: [&dyn ToSqlTurso; 1] = [&user_id];
    let wrows = db
        .query("SELECT id FROM workspaces WHERE owner_user_id=? LIMIT 1", &p2)
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let workspace_id = wrows.first().map(|r| get_str(r, "id")).unwrap_or_default();

    let token = new_token();
    let created = now_str();
    let expires = (Utc::now() + Duration::days(SESSION_TTL_DAYS))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let ip: [&dyn ToSqlTurso; 7] =
        [&token, &user_id, &email, &name, &workspace_id, &created, &expires];
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
            .query("SELECT value FROM kv_settings WHERE workspace_id=? AND key=?", &p)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let cfg: Value = match rows.first().map(|r| get_str(r, "value")) {
            Some(s) if !s.is_empty() => serde_json::from_str(&s).unwrap_or(json!({"campaigns": []})),
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
            Err(_) => return Err((StatusCode::BAD_REQUEST, Json(json!({"error":"config が不正です"})))),
        },
        None => return Err((StatusCode::BAD_REQUEST, Json(json!({"error":"config が必要です"})))),
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

fn config_with_sections(sections: &Value) -> Result<Value, CoreErr> {
    let updates = sections
        .as_object()
        .ok_or_else(|| cerr(StatusCode::BAD_REQUEST, "sections はobjectが必要です"))?;
    let mut config: Value = serde_json::from_str(DEFAULT_CONFIG_JSON)
        .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("既定config不正: {e}")))?;
    let target = config
        .as_object_mut()
        .ok_or_else(|| cerr(StatusCode::INTERNAL_SERVER_ERROR, "既定configがobjectではありません"))?;
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
    let patch_str = serde_json::to_string(&sections)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error":"sections が不正です"}))))?;
    let initial_str = serde_json::to_string(
        &config_with_sections(&sections).map_err(|(code, message)| {
            (code, Json(json!({"error": message})))
        })?,
    )
    .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error":"sections が不正です"}))))?;
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
            .query("SELECT value FROM kv_settings WHERE workspace_id=? AND key=?", &q)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let value = rows
            .first()
            .map(|row| get_str(row, "value"))
            .ok_or_else(|| cerr(StatusCode::INTERNAL_SERVER_ERROR, "更新後configがありません"))?;
        let config = serde_json::from_str::<Value>(&value)
            .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("更新後config不正: {e}")))?;
        Ok(json!({ "ok": true, "config": config }))
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
            .query("SELECT value FROM kv_settings WHERE workspace_id=? AND key=?", &p)
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
            Err(_) => return Err((StatusCode::BAD_REQUEST, Json(json!({"error":"state が不正です"})))),
        },
        None => return Err((StatusCode::BAD_REQUEST, Json(json!({"error":"state が必要です"})))),
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
    let campaign_id = body.get("campaign_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let candidate = body.get("candidate_web_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let platform = body.get("platform").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let subject_chars = body.get("subject_chars").and_then(|v| v.as_i64()).unwrap_or(0);
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
        let pi: [&dyn ToSqlTurso; 7] = [
            &campaign_id,
            &candidate,
            &platform,
            &now,
            &subject_chars,
            &body_chars,
            &u.workspace_id,
        ];
        dbh.execute(
            "INSERT INTO send_history(campaign_id,candidate_web_id,platform,sent_at,subject_chars,body_chars,workspace_id) VALUES(?,?,?,?,?,?,?)",
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
        return Err((StatusCode::FORBIDDEN, Json(json!({"error":"管理者トークンが必要です"}))));
    }
    // scope 省略時は 'global'（全体停止）。
    let scope = {
        let s = body.get("scope").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if s.is_empty() { "global".to_string() } else { s }
    };
    let disabled = if body.get("disabled").and_then(|v| v.as_bool()).unwrap_or(true) { 1i64 } else { 0i64 };
    let reason = body.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string();
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
        return Err((StatusCode::FORBIDDEN, Json(json!({"error":"管理者トークンが必要です"}))));
    }
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("").trim().to_lowercase();
    let disabled = if body.get("disabled").and_then(|v| v.as_bool()).unwrap_or(true) { 1i64 } else { 0i64 };
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
        return Err((StatusCode::FORBIDDEN, Json(json!({"error":"管理者トークンが必要です"}))));
    }
    let company = body.get("company").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("").trim().to_lowercase();
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let name_in = body.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let name = if name_in.is_empty() { company.clone() } else { name_in };
    // role は master/member のみ許可。既定は member(招待された実務者)。master 昇格は明示指定。
    let role = match body.get("role").and_then(|v| v.as_str()).unwrap_or("member").trim() {
        "master" => "master".to_string(),
        _ => "member".to_string(),
    };
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || provision_core(&dbh, company, email, password, name, role)).await
}

fn provision_core(
    db: &TursoDb,
    company: String,
    email: String,
    password: String,
    name: String,
    role: String,
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

    let user_id = new_id();
    let ws_id = new_id();
    let now = now_str();
    let hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST)
        .map_err(|_| cerr(StatusCode::INTERNAL_SERVER_ERROR, "ハッシュ生成失敗"))?;

    let pw: [&dyn ToSqlTurso; 4] = [&ws_id, &company, &user_id, &now];
    db.execute(
        "INSERT INTO workspaces(id,name,owner_user_id,created_at) VALUES(?,?,?,?)",
        &pw,
    )
    .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let pu: [&dyn ToSqlTurso; 6] = [&user_id, &email, &hash, &name, &now, &role];
    db.execute(
        "INSERT INTO users(id,email,password_hash,name,created_at,role) VALUES(?,?,?,?,?,?)",
        &pu,
    )
    .map_err(|e| cerr(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let key = CONFIG_KEY.to_string();
    let cfg = DEFAULT_CONFIG_JSON.to_string();
    let pc: [&dyn ToSqlTurso; 3] = [&ws_id, &key, &cfg];
    let _ = db.execute(
        "INSERT OR REPLACE INTO kv_settings(workspace_id,key,value) VALUES(?,?,?)",
        &pc,
    );

    Ok(json!({
        "ok": true,
        "company": company,
        "email": email,
        "workspace_id": ws_id,
        "role": role,
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
    let company = body.get("company").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("").trim().to_lowercase();
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let name_in = body.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let name = if name_in.is_empty() { company.clone() } else { name_in };
    let role = match body.get("role").and_then(|v| v.as_str()).unwrap_or("member").trim() {
        "master" => "master".to_string(),
        _ => "member".to_string(),
    };
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        require_master(&dbh, &token)?;
        provision_core(&dbh, company, email, password, name, role)
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
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("").trim().to_lowercase();
    let new_password = body.get("new_password").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let dbh = match take_db(&state) {
        Ok(d) => d,
        Err((c, m)) => return Err((c, Json(json!({ "error": m })))),
    };
    run(move || {
        require_master(&dbh, &token)?;
        if email.is_empty() || new_password.len() < 8 {
            return Err(cerr(StatusCode::BAD_REQUEST, "email・8文字以上の new_password が必要です"));
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
