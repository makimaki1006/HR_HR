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
    if crate::crm::email::tracking::enabled_from_env() {
        crate::crm::email::tracking::ensure_tables(turso)?;
    }
    tracing::info!(
        "audit tables ensured (accounts/login_sessions/activity_logs/crm_pending_operations/job_copy_draft_operations)"
    );
    Ok(())
}

const TABLE_DDL: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS crm_saved_views (
        id TEXT PRIMARY KEY, creator_email TEXT NOT NULL, name TEXT NOT NULL,
        scope TEXT NOT NULL CHECK(scope IN ('private','shared')), query TEXT NOT NULL,
        position REAL NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
    )"#,
    "CREATE INDEX IF NOT EXISTS crm_saved_views_visibility ON crm_saved_views(scope, creator_email)",
    // ADR-028: 架電画面の成績の帯に出す項目の個人設定 (1 人 1 行、JSON 文字列)。HubSpot の業務データは持たない。
    r#"CREATE TABLE IF NOT EXISTS crm_metric_prefs (
        email TEXT PRIMARY KEY, prefs TEXT NOT NULL, updated_at TEXT NOT NULL
    )"#,
    r#"CREATE TABLE IF NOT EXISTS crm_role_property_layouts (
        role TEXT NOT NULL, object_type TEXT NOT NULL, properties TEXT NOT NULL,
        updated_by TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(role, object_type)
    )"#,
    // ADR-022: per-operator Gmail sync cursor only, not mailbox/business content.
    r#"CREATE TABLE IF NOT EXISTS crm_gmail_reply_cursors (
        operator_email TEXT PRIMARY KEY, history_id TEXT NOT NULL
    )"#,
    // ADR-022: reply deduplication/leases and consent health, not a CRM history copy.
    r#"CREATE TABLE IF NOT EXISTS crm_gmail_reply_receipts (
        message_id TEXT PRIMARY KEY, operation_id TEXT NOT NULL UNIQUE,
        claimed_by TEXT NOT NULL DEFAULT '', claim_until TEXT NOT NULL DEFAULT ''
    )"#,
    r#"CREATE TABLE IF NOT EXISTS crm_gmail_reply_status (
        operator_email TEXT PRIMARY KEY, last_checked_at TEXT, last_imported_at TEXT,
        expired_grant_at TEXT
    )"#,
    // Gmail credentials only; email business history remains in HubSpot.
    r#"CREATE TABLE IF NOT EXISTS crm_gmail_grants (
        operator_email TEXT PRIMARY KEY, encrypted_refresh_token TEXT NOT NULL,
        scopes TEXT NOT NULL, granted_at TEXT NOT NULL, revoked_at TEXT
    )"#,
    // ADR-019: 求人本文の複製DBではなく、書き込みの再送と冪等性だけの台帳。
    r#"CREATE TABLE IF NOT EXISTS job_copy_draft_operations (
        operation_id TEXT PRIMARY KEY, operator_email TEXT NOT NULL, listing_id TEXT NOT NULL,
        payload TEXT NOT NULL, status TEXT NOT NULL, next_retry_at TEXT NOT NULL, updated_at TEXT NOT NULL
    )"#,
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
    // crm_pending_operations: Headless CRM の書き込み (PATCH /api/crm/deals/{id}) の操作台帳 (ADR-018)。
    //   CRM のコピーではなく、HubSpot へ「まだ反映されていない / 反映した結果を再送時に返すための」操作だけを持つ。
    //   status: in_progress (送信中) / pending (一時障害で再送待ち) / saved / failed / discarded
    //   保存済み (saved) の行は冪等性 (同じ operation_id の再送で同じ結果を返す) のため数日残し、worker が消す。
    r#"CREATE TABLE IF NOT EXISTS crm_pending_operations (
        operation_id    TEXT PRIMARY KEY,
        operator_email  TEXT NOT NULL,
        deal_id         TEXT NOT NULL,
        object_refs     TEXT,
        payload         TEXT NOT NULL,
        status          TEXT NOT NULL,
        attempts        INTEGER NOT NULL DEFAULT 0,
        last_error_code TEXT,
        http_status     INTEGER,
        result_json     TEXT,
        next_retry_at   TEXT,
        created_at      TEXT NOT NULL,
        updated_at      TEXT NOT NULL
    )"#,
];

const INDEX_DDL: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_job_copy_draft_retry ON job_copy_draft_operations(status, next_retry_at)",
    "CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_account_started ON login_sessions(account_id, started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_started ON login_sessions(started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_activity_account_at ON activity_logs(account_id, at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_activity_at ON activity_logs(at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_crm_ops_status_retry ON crm_pending_operations(status, next_retry_at)",
];
