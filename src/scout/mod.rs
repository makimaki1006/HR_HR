//! Scout（スカウト自動化）中央バックエンド
//!
//! OpenWorkScoutRPA のローカルアプリ（顧客PC）が HTTP 経由で叩く API を `/scout/*` に提供する。
//! - データは専用 Turso DB（`SCOUT_TURSO_URL`/`SCOUT_TURSO_TOKEN`）に保存。HR_HR 本体のDBには触れない。
//! - 認証は HR_HR の cookie/ドメイン認証とは独立した「トークン認証（`auth_sessions` テーブル）」。
//!   → `/scout/*` は HR_HR の require_auth をバイパスし、各エンドポイントで自前トークンを検証する。
//!
//! 本モジュールは段階実装中。まずは health（疎通確認）のみ。認証/データAPIは順次追加。

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::AppState;

/// `/scout/*` のルーター。build_app で merge される。
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/scout/api/health", get(health))
}

/// 疎通確認。scout 用 Turso DB が接続済みかも返す（認証不要）。
async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "scout-backend",
        "db_connected": state.scout_db.is_some(),
    }))
}
