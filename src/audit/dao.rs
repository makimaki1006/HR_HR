//! 監査DB DAO レイヤ
//!
//! すべての関数は純粋な SQL 実行ラッパ。
//! エラーは tracing::warn に流しつつ戻り値は Result で返す。
//! 上位ハンドラは `let _ = ...` で無視することで本番影響を避ける設計。

use super::{new_uuid, now_iso8601, AuditDb};
use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::{get_i64, get_str};
use serde::Serialize;

#[derive(Debug, Default, Clone, Serialize)]
pub struct AccountRow {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub company: String,
    pub role: String,
    pub first_seen_at: String,
    pub last_login_at: String,
    pub login_count: i64,
    pub disabled_at: String,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct LoginSessionRow {
    pub id: String,
    pub account_id: String,
    pub attempted_email: String,
    pub started_at: String,
    pub ended_at: String,
    pub ip_hash: String,
    pub user_agent: String,
    pub login_method: String,
    pub success: i64,
    pub failure_reason: String,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ActivityLogRow {
    pub id: String,
    pub account_id: String,
    pub session_id: String,
    pub at: String,
    pub event_type: String,
    pub target_type: String,
    pub target_id: String,
    pub meta: String,
}

// ============================================================================
// accounts
// ============================================================================

/// email をキーにアカウントを upsert し、`account_id` を返す。
///
/// - 未登録 → INSERT (role='user', login_count=1, first_seen/last_login=now)
/// - 登録済 → UPDATE last_login_at, login_count += 1
///
/// Returns: accounts.id (UUID)
pub fn upsert_account(
    audit: &AuditDb,
    email: &str,
    admin_emails: &[String],
) -> Result<String, String> {
    let turso = audit.turso();
    let now = now_iso8601();
    let role = if admin_emails.iter().any(|a| a.eq_ignore_ascii_case(email)) {
        "admin"
    } else {
        "user"
    };

    // 既存検索
    let rows = turso.query(
        "SELECT id, role FROM accounts WHERE email = ?1 LIMIT 1",
        &[&email],
    )?;

    if let Some(row) = rows.first() {
        let id = get_str(row, "id");
        let existing_role = get_str(row, "role");
        // 管理者昇格が必要なら role も更新
        if existing_role != role && role == "admin" {
            let _ = turso.execute(
                "UPDATE accounts SET role = ?1, last_login_at = ?2, login_count = login_count + 1 WHERE id = ?3",
                &[&role, &now, &id],
            );
        } else {
            let _ = turso.execute(
                "UPDATE accounts SET last_login_at = ?1, login_count = login_count + 1 WHERE id = ?2",
                &[&now, &id],
            );
        }
        return Ok(id);
    }

    // 新規登録
    let id = new_uuid();
    turso.execute(
        "INSERT INTO accounts (id, email, role, first_seen_at, last_login_at, login_count) \
         VALUES (?1, ?2, ?3, ?4, ?4, 1)",
        &[&id, &email, &role, &now],
    )?;
    tracing::info!(email = %email, role = %role, "account auto-provisioned");
    Ok(id)
}

/// アカウント全件取得 (管理者画面用、最大 limit 件)。
pub fn list_accounts(turso: &TursoDb, limit: i64) -> Vec<AccountRow> {
    let rows = turso
        .query(
            "SELECT id, email, display_name, company, role, first_seen_at, \
                    last_login_at, login_count, disabled_at \
             FROM accounts ORDER BY last_login_at DESC NULLS LAST LIMIT ?1",
            &[&limit],
        )
        .unwrap_or_default();
    rows.into_iter().map(row_to_account).collect()
}

/// email で 1 件取得
pub fn find_account_by_email(turso: &TursoDb, email: &str) -> Option<AccountRow> {
    let rows = turso
        .query(
            "SELECT id, email, display_name, company, role, first_seen_at, \
                    last_login_at, login_count, disabled_at \
             FROM accounts WHERE email = ?1 LIMIT 1",
            &[&email],
        )
        .ok()?;
    rows.into_iter().next().map(row_to_account)
}

/// id で 1 件取得
pub fn find_account_by_id(turso: &TursoDb, id: &str) -> Option<AccountRow> {
    let rows = turso
        .query(
            "SELECT id, email, display_name, company, role, first_seen_at, \
                    last_login_at, login_count, disabled_at \
             FROM accounts WHERE id = ?1 LIMIT 1",
            &[&id],
        )
        .ok()?;
    rows.into_iter().next().map(row_to_account)
}

/// プロフィール自己編集 (display_name / company のみ)
pub fn update_profile(
    turso: &TursoDb,
    account_id: &str,
    display_name: &str,
    company: &str,
) -> Result<(), String> {
    turso.execute(
        "UPDATE accounts SET display_name = ?1, company = ?2 WHERE id = ?3",
        &[&display_name, &company, &account_id],
    )
}

fn row_to_account(r: std::collections::HashMap<String, serde_json::Value>) -> AccountRow {
    AccountRow {
        id: get_str(&r, "id"),
        email: get_str(&r, "email"),
        display_name: get_str(&r, "display_name"),
        company: get_str(&r, "company"),
        role: get_str(&r, "role"),
        first_seen_at: get_str(&r, "first_seen_at"),
        last_login_at: get_str(&r, "last_login_at"),
        login_count: get_i64(&r, "login_count"),
        disabled_at: get_str(&r, "disabled_at"),
    }
}

// ============================================================================
// login_sessions
// ============================================================================

/// ログイン成功時: 新しい login_session を作成し、その session_id を返す
pub fn insert_login_session(
    audit: &AuditDb,
    account_id: &str,
    ip_hash: &str,
    user_agent: &str,
    login_method: &str,
) -> Result<String, String> {
    let id = new_uuid();
    let now = now_iso8601();
    audit.turso().execute(
        "INSERT INTO login_sessions \
         (id, account_id, started_at, ip_hash, user_agent, login_method, success) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
        &[&id, &account_id, &now, &ip_hash, &user_agent, &login_method],
    )?;
    Ok(id)
}

/// ログアウト時: ended_at を埋める
pub fn mark_session_ended(audit: &AuditDb, session_id: &str) -> Result<(), String> {
    let now = now_iso8601();
    audit.turso().execute(
        "UPDATE login_sessions SET ended_at = ?1 WHERE id = ?2 AND ended_at IS NULL",
        &[&now, &session_id],
    )
}

/// ログイン失敗を記録 (account_id=NULL、attempted_email に生で残す)
pub fn log_failed_login(
    audit: &AuditDb,
    attempted_email: &str,
    ip_hash: &str,
    user_agent: &str,
    login_method: &str,
    failure_reason: &str,
) -> Result<(), String> {
    let id = new_uuid();
    let now = now_iso8601();
    audit.turso().execute(
        "INSERT INTO login_sessions \
         (id, attempted_email, started_at, ip_hash, user_agent, login_method, success, failure_reason) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        &[&id, &attempted_email, &now, &ip_hash, &user_agent, &login_method, &failure_reason],
    )
}

/// 特定アカウントの最近の login_sessions (成功・失敗両方)
pub fn list_sessions_for_account(
    turso: &TursoDb,
    account_id: &str,
    limit: i64,
) -> Vec<LoginSessionRow> {
    turso
        .query(
            "SELECT id, account_id, attempted_email, started_at, ended_at, ip_hash, \
                    user_agent, login_method, success, failure_reason \
             FROM login_sessions WHERE account_id = ?1 ORDER BY started_at DESC LIMIT ?2",
            &[&account_id, &limit],
        )
        .unwrap_or_default()
        .into_iter()
        .map(row_to_session)
        .collect()
}

/// 失敗ログだけ抽出 (管理者監視画面用)
pub fn list_recent_failures(turso: &TursoDb, limit: i64) -> Vec<LoginSessionRow> {
    turso
        .query(
            "SELECT id, account_id, attempted_email, started_at, ended_at, ip_hash, \
                    user_agent, login_method, success, failure_reason \
             FROM login_sessions WHERE success = 0 ORDER BY started_at DESC LIMIT ?1",
            &[&limit],
        )
        .unwrap_or_default()
        .into_iter()
        .map(row_to_session)
        .collect()
}

fn row_to_session(r: std::collections::HashMap<String, serde_json::Value>) -> LoginSessionRow {
    LoginSessionRow {
        id: get_str(&r, "id"),
        account_id: get_str(&r, "account_id"),
        attempted_email: get_str(&r, "attempted_email"),
        started_at: get_str(&r, "started_at"),
        ended_at: get_str(&r, "ended_at"),
        ip_hash: get_str(&r, "ip_hash"),
        user_agent: get_str(&r, "user_agent"),
        login_method: get_str(&r, "login_method"),
        success: get_i64(&r, "success"),
        failure_reason: get_str(&r, "failure_reason"),
    }
}

// ============================================================================
// activity_logs
// ============================================================================

/// 重要操作を1件記録 (fire-and-forget。失敗しても本番動作に影響させない)
pub fn insert_activity(
    audit: &AuditDb,
    account_id: &str,
    session_id: &str,
    event_type: &str,
    target_type: &str,
    target_id: &str,
    meta: &str,
) {
    let id = new_uuid();
    let at = now_iso8601();
    let result = audit.turso().execute(
        "INSERT INTO activity_logs \
         (id, account_id, session_id, at, event_type, target_type, target_id, meta) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        &[
            &id,
            &account_id,
            &session_id,
            &at,
            &event_type,
            &target_type,
            &target_id,
            &meta,
        ],
    );
    if let Err(e) = result {
        tracing::warn!(
            event = %event_type,
            account_id = %account_id,
            "activity log failed: {e}"
        );
    }
}

/// 特定アカウントの最近のアクティビティ
pub fn list_activity_for_account(
    turso: &TursoDb,
    account_id: &str,
    limit: i64,
) -> Vec<ActivityLogRow> {
    turso
        .query(
            "SELECT id, account_id, session_id, at, event_type, target_type, target_id, meta \
             FROM activity_logs WHERE account_id = ?1 ORDER BY at DESC LIMIT ?2",
            &[&account_id, &limit],
        )
        .unwrap_or_default()
        .into_iter()
        .map(row_to_activity)
        .collect()
}

/// 1年より古いログを削除 (日次バッチから呼ぶ)
pub fn purge_old_activity(audit: &AuditDb) -> Result<(), String> {
    // 365日前の ISO8601
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(365))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    audit
        .turso()
        .execute("DELETE FROM activity_logs WHERE at < ?1", &[&cutoff])?;
    audit.turso().execute(
        "DELETE FROM login_sessions WHERE started_at < ?1",
        &[&cutoff],
    )?;
    tracing::info!(cutoff = %cutoff, "purged activity/sessions older than 1 year");
    Ok(())
}

fn row_to_activity(r: std::collections::HashMap<String, serde_json::Value>) -> ActivityLogRow {
    ActivityLogRow {
        id: get_str(&r, "id"),
        account_id: get_str(&r, "account_id"),
        session_id: get_str(&r, "session_id"),
        at: get_str(&r, "at"),
        event_type: get_str(&r, "event_type"),
        target_type: get_str(&r, "target_type"),
        target_id: get_str(&r, "target_id"),
        meta: get_str(&r, "meta"),
    }
}
