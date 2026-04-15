//! 監査DBテーブル初期化 (CREATE TABLE IF NOT EXISTS)
//!
//! アプリ起動時に1度呼ぶ。冪等なので複数回呼んでも安全。

use crate::db::turso_http::TursoDb;

/// 全テーブル + インデックスを作成
pub fn ensure_audit_tables(turso: &TursoDb) -> Result<(), String> {
    for sql in TABLE_DDL.iter().chain(INDEX_DDL.iter()) {
        turso
            .execute(sql, &[])
            .map_err(|e| format!("audit schema failed on `{sql}`: {e}"))?;
    }
    tracing::info!("audit tables ensured (accounts/login_sessions/activity_logs)");
    Ok(())
}

const TABLE_DDL: &[&str] = &[
    // accounts: メール単位で 1 行。password_hash は持たず、既存の config ベース
    //           認証結果に対して upsert するだけの台帳。
    r#"CREATE TABLE IF NOT EXISTS accounts (
        id             TEXT PRIMARY KEY,
        email          TEXT NOT NULL UNIQUE,
        display_name   TEXT,
        company        TEXT,
        role           TEXT NOT NULL DEFAULT 'user',
        first_seen_at  TEXT NOT NULL,
        last_login_at  TEXT,
        login_count    INTEGER NOT NULL DEFAULT 0,
        disabled_at    TEXT
    )"#,
    // login_sessions: 成功/失敗の全試行を残す。失敗時は account_id=NULL、
    //                 attempted_email にメールを生で残して不正検知に備える。
    r#"CREATE TABLE IF NOT EXISTS login_sessions (
        id               TEXT PRIMARY KEY,
        account_id       TEXT,
        attempted_email  TEXT,
        started_at       TEXT NOT NULL,
        ended_at         TEXT,
        ip_hash          TEXT,
        user_agent       TEXT,
        login_method     TEXT NOT NULL,
        success          INTEGER NOT NULL,
        failure_reason   TEXT
    )"#,
    // activity_logs: 重要操作のみ記録。1年経過後は purge_old_activity で削除。
    r#"CREATE TABLE IF NOT EXISTS activity_logs (
        id          TEXT PRIMARY KEY,
        account_id  TEXT NOT NULL,
        session_id  TEXT,
        at          TEXT NOT NULL,
        event_type  TEXT NOT NULL,
        target_type TEXT,
        target_id   TEXT,
        meta        TEXT
    )"#,
];

const INDEX_DDL: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_account_started ON login_sessions(account_id, started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_started ON login_sessions(started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_activity_account_at ON activity_logs(account_id, at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_activity_at ON activity_logs(at DESC)",
];
