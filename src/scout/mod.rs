//! Scout（スカウト自動化）中央バックエンド
//!
//! OpenWorkScoutRPA のローカルアプリ（顧客PC）が HTTP 経由で叩く API を `/scout/*` に提供する。
//! - データは専用 Turso DB（`SCOUT_TURSO_URL`/`SCOUT_TURSO_TOKEN`）に保存。HR_HR 本体DBには触れない。
//! - 認証は HR_HR の cookie/ドメイン認証とは独立した「トークン認証」。`/scout/*` は
//!   HR_HR の require_auth/CSRF をバイパスし、各エンドポイントで自前トークンを検証する。
//! - パスワードは bcrypt（HR_HR 既存依存を再利用）。
//! - config は workspace ごとの JSON ドキュメントとして kv_settings に保存（ローカルアプリはJSONで受領）。

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::State,
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
        .route("/scout/api/config", get(get_config).post(save_config))
        .route("/scout/api/admin/provision", post(provision))
}

// ===== 共通ヘルパー =====

type ApiErr = (StatusCode, Json<Value>);
type ApiResult = Result<Json<Value>, ApiErr>;

fn err(code: StatusCode, msg: &str) -> ApiErr {
    (code, Json(json!({ "error": msg })))
}

fn db(state: &AppState) -> Result<&TursoDb, ApiErr> {
    state
        .scout_db
        .as_ref()
        .ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "scout DB が未設定です"))
}

/// 文字列比較で期限判定できるよう、UTCの固定フォーマットに揃える。
fn now_str() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn get_str(row: &HashMap<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
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
}

/// トークンからログイン中ユーザーを解決（期限切れは None）。
fn current_user(dbh: &TursoDb, token: &str) -> Option<SessionUser> {
    if token.is_empty() {
        return None;
    }
    let t = token.to_string();
    let params: [&dyn ToSqlTurso; 1] = [&t];
    let rows = dbh
        .query(
            "SELECT user_id,email,name,workspace_id,expires_at FROM auth_sessions WHERE token=?",
            &params,
        )
        .ok()?;
    let r = rows.first()?;
    let exp = get_str(r, "expires_at");
    if exp.as_str() <= now_str().as_str() {
        return None;
    }
    Some(SessionUser {
        user_id: get_str(r, "user_id"),
        email: get_str(r, "email"),
        name: get_str(r, "name"),
        workspace_id: get_str(r, "workspace_id"),
    })
}

fn require_user(state: &AppState, headers: &HeaderMap) -> Result<SessionUser, ApiErr> {
    let dbh = db(state)?;
    let token = token_from(headers);
    current_user(dbh, &token)
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "未ログイン(トークンが無効です)"))
}

// ===== エンドポイント =====

async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "scout-backend",
        "db_connected": state.scout_db.is_some(),
    }))
}

async fn login(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> ApiResult {
    let dbh = db(&state)?;
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("");
    if email.is_empty() || password.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "メールとパスワードが必要です"));
    }

    let params: [&dyn ToSqlTurso; 1] = [&email];
    let rows = dbh
        .query(
            "SELECT id,email,password_hash,name FROM users WHERE email=?",
            &params,
        )
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {e}")))?;
    let row = match rows.first() {
        Some(r) => r,
        None => return Err(err(StatusCode::UNAUTHORIZED, "メールまたはパスワードが違います")),
    };
    let hash = get_str(row, "password_hash");
    let ok = bcrypt::verify(password, &hash).unwrap_or(false);
    if !ok {
        return Err(err(StatusCode::UNAUTHORIZED, "メールまたはパスワードが違います"));
    }

    let user_id = get_str(row, "id");
    let name = get_str(row, "name");

    // workspace 解決 (owner_user_id)
    let p2: [&dyn ToSqlTurso; 1] = [&user_id];
    let wrows = dbh
        .query("SELECT id FROM workspaces WHERE owner_user_id=? LIMIT 1", &p2)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {e}")))?;
    let workspace_id = wrows.first().map(|r| get_str(r, "id")).unwrap_or_default();

    // セッション発行
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
    dbh.execute(
        "INSERT INTO auth_sessions(token,user_id,email,name,workspace_id,created_at,expires_at) VALUES(?,?,?,?,?,?,?)",
        &ip,
    )
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {e}")))?;

    Ok(Json(json!({
        "ok": true,
        "token": token,
        "user": {"email": email, "name": name, "workspace_id": workspace_id},
    })))
}

async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let u = require_user(&state, &headers)?;
    Ok(Json(json!({
        "user": {"user_id": u.user_id, "email": u.email, "name": u.name, "workspace_id": u.workspace_id}
    })))
}

async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let dbh = db(&state)?;
    let token = token_from(&headers);
    if !token.is_empty() {
        let p: [&dyn ToSqlTurso; 1] = [&token];
        let _ = dbh.execute("DELETE FROM auth_sessions WHERE token=?", &p);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn get_config(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    let u = require_user(&state, &headers)?;
    let dbh = db(&state)?;
    let ws = u.workspace_id.clone();
    let key = CONFIG_KEY.to_string();
    let p: [&dyn ToSqlTurso; 2] = [&ws, &key];
    let rows = dbh
        .query("SELECT value FROM kv_settings WHERE workspace_id=? AND key=?", &p)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {e}")))?;
    let cfg: Value = match rows.first().map(|r| get_str(r, "value")) {
        Some(s) if !s.is_empty() => serde_json::from_str(&s).unwrap_or(json!({"campaigns": []})),
        _ => serde_json::from_str(DEFAULT_CONFIG_JSON).unwrap_or(json!({"campaigns": []})),
    };
    Ok(Json(json!({ "config": cfg })))
}

async fn save_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let u = require_user(&state, &headers)?;
    let dbh = db(&state)?;
    let cfg = body
        .get("config")
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "config が必要です"))?;
    let cfg_str =
        serde_json::to_string(cfg).map_err(|_| err(StatusCode::BAD_REQUEST, "config が不正です"))?;
    let ws = u.workspace_id.clone();
    let key = CONFIG_KEY.to_string();
    let p: [&dyn ToSqlTurso; 3] = [&ws, &key, &cfg_str];
    dbh.execute(
        "INSERT OR REPLACE INTO kv_settings(workspace_id,key,value) VALUES(?,?,?)",
        &p,
    )
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {e}")))?;
    Ok(Json(json!({ "ok": true })))
}

/// 管理者プロビジョニング（招待制）。env `SCOUT_ADMIN_TOKEN` を持つ管理者のみ。
async fn provision(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult {
    let admin_token = std::env::var("SCOUT_ADMIN_TOKEN").unwrap_or_default();
    if admin_token.is_empty()
        || headers.get("x-admin-token").and_then(|v| v.to_str().ok()) != Some(admin_token.as_str())
    {
        return Err(err(StatusCode::FORBIDDEN, "管理者トークンが必要です"));
    }
    let dbh = db(&state)?;
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
    let password = body.get("password").and_then(|v| v.as_str()).unwrap_or("");
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
    if company.is_empty() || email.is_empty() || password.len() < 8 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "company・email・8文字以上のpassword が必要です",
        ));
    }

    // 重複チェック
    let pe: [&dyn ToSqlTurso; 1] = [&email];
    let exists = dbh
        .query("SELECT 1 AS x FROM users WHERE email=?", &pe)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {e}")))?;
    if !exists.is_empty() {
        return Err(err(StatusCode::CONFLICT, "既に登録済みのメールです"));
    }

    let user_id = new_id();
    let ws_id = new_id();
    let now = now_str();
    let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "ハッシュ生成失敗"))?;

    let pw: [&dyn ToSqlTurso; 4] = [&ws_id, &company, &user_id, &now];
    dbh.execute(
        "INSERT INTO workspaces(id,name,owner_user_id,created_at) VALUES(?,?,?,?)",
        &pw,
    )
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {e}")))?;
    let pu: [&dyn ToSqlTurso; 5] = [&user_id, &email, &hash, &name, &now];
    dbh.execute(
        "INSERT INTO users(id,email,password_hash,name,created_at) VALUES(?,?,?,?,?)",
        &pu,
    )
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {e}")))?;
    // 既定 config をシード
    let key = CONFIG_KEY.to_string();
    let cfg = DEFAULT_CONFIG_JSON.to_string();
    let pc: [&dyn ToSqlTurso; 3] = [&ws_id, &key, &cfg];
    let _ = dbh.execute(
        "INSERT OR REPLACE INTO kv_settings(workspace_id,key,value) VALUES(?,?,?)",
        &pc,
    );

    Ok(Json(json!({
        "ok": true,
        "company": company,
        "email": email,
        "workspace_id": ws_id,
    })))
}
