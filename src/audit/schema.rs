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
    // ADR-033: トークの振り返り。1 行 = 1 Call の判定結果。文字起こしの全文は持たない (画面で Zoom から取り直す)。
    // status: pending_transcript (文字起こし待ち) / judged / unavailable (録音・文字起こしが取れない) / failed
    r#"CREATE TABLE IF NOT EXISTS call_talk_reviews (
        hubspot_call_id TEXT PRIMARY KEY, owner_id TEXT NOT NULL, call_at TEXT NOT NULL,
        duration_sec INTEGER NOT NULL, zoom_call_log_id TEXT, status TEXT NOT NULL,
        attempts INTEGER NOT NULL DEFAULT 0, next_retry_at TEXT, checks_version TEXT, model TEXT,
        outcome TEXT, reached_person INTEGER, reception INTEGER, first_date_turn INTEGER,
        items TEXT, summary TEXT, note TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
    )"#,
    // ADR-032: empty deal/key is the daily completion marker, including zero flags.
    r#"CREATE TABLE IF NOT EXISTS consulting_deal_flags (
        data_day TEXT NOT NULL, deal_id TEXT NOT NULL, flag_key TEXT NOT NULL,
        since_day TEXT NOT NULL, PRIMARY KEY(data_day, deal_id, flag_key)
    )"#,
    // コンサルKPI: 顧客情報を持たない、データの日ごとの担当者別件数。
    r#"CREATE TABLE IF NOT EXISTS consulting_team_counts (
        data_day TEXT NOT NULL, consultant TEXT NOT NULL, metric TEXT NOT NULL,
        count INTEGER NOT NULL CHECK(count >= 0),
        PRIMARY KEY(data_day, consultant, metric)
    )"#,
    // ADR-029: 再生成可能な一覧のキャッシュ。最後の分割行と旧世代の削除は同一文で確定する。
    r#"CREATE TABLE IF NOT EXISTS job_copy_index_snapshots (
        generation TEXT NOT NULL, version INTEGER NOT NULL, built_at TEXT NOT NULL,
        part INTEGER NOT NULL, total INTEGER NOT NULL CHECK(total BETWEEN 1 AND 128),
        payload TEXT NOT NULL, PRIMARY KEY(generation, part),
        CHECK(part >= 0 AND part < total)
    )"#,
    r#"CREATE TRIGGER IF NOT EXISTS job_copy_index_snapshot_complete
    AFTER INSERT ON job_copy_index_snapshots
    WHEN NEW.part = NEW.total - 1 AND
        (SELECT COUNT(*) FROM job_copy_index_snapshots WHERE generation = NEW.generation) = NEW.total
    BEGIN
        DELETE FROM job_copy_index_snapshots WHERE generation <> NEW.generation;
    END"#,
    // ADR-030: disposable reports; preserve the previous generation until the final part.
    r#"CREATE TABLE IF NOT EXISTS job_copy_recent_snapshots (
        generation TEXT NOT NULL, version INTEGER NOT NULL, built_at TEXT NOT NULL,
        part INTEGER NOT NULL, total INTEGER NOT NULL CHECK(total BETWEEN 1 AND 128),
        payload TEXT NOT NULL, PRIMARY KEY(generation, part),
        CHECK(part >= 0 AND part < total)
    )"#,
    r#"CREATE TRIGGER IF NOT EXISTS job_copy_recent_snapshots_complete
    AFTER INSERT ON job_copy_recent_snapshots
    WHEN NEW.part = NEW.total - 1 AND
        (SELECT COUNT(*) FROM job_copy_recent_snapshots WHERE generation = NEW.generation) = NEW.total
    BEGIN
        DELETE FROM job_copy_recent_snapshots WHERE generation <> NEW.generation;
    END"#,
    r#"CREATE TABLE IF NOT EXISTS job_copy_todo_fact_snapshots (
        generation TEXT NOT NULL, version INTEGER NOT NULL, built_at TEXT NOT NULL,
        part INTEGER NOT NULL, total INTEGER NOT NULL CHECK(total BETWEEN 1 AND 128),
        payload TEXT NOT NULL, PRIMARY KEY(generation, part),
        CHECK(part >= 0 AND part < total)
    )"#,
    r#"CREATE TRIGGER IF NOT EXISTS job_copy_todo_fact_snapshots_complete
    AFTER INSERT ON job_copy_todo_fact_snapshots
    WHEN NEW.part = NEW.total - 1 AND
        (SELECT COUNT(*) FROM job_copy_todo_fact_snapshots WHERE generation = NEW.generation) = NEW.total
    BEGIN
        DELETE FROM job_copy_todo_fact_snapshots WHERE generation <> NEW.generation;
    END"#,
    // ADR-032: 案件管理の記録欄 (検証記録・要因メモ・外部文面)。求人 ID x 欄の種類で 1 行。HubSpot の業務データは持たない。
    r#"CREATE TABLE IF NOT EXISTS job_copy_listing_notes (
        listing_id TEXT NOT NULL, field TEXT NOT NULL CHECK(field IN ('review','factor','incoming')),
        value TEXT NOT NULL, updated_by TEXT NOT NULL, updated_at TEXT NOT NULL,
        PRIMARY KEY(listing_id, field)
    )"#,
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
    "CREATE INDEX IF NOT EXISTS idx_call_talk_reviews_call_at ON call_talk_reviews(call_at)",
    "CREATE INDEX IF NOT EXISTS idx_call_talk_reviews_retry ON call_talk_reviews(status, next_retry_at)",
    "CREATE INDEX IF NOT EXISTS idx_job_copy_draft_retry ON job_copy_draft_operations(status, next_retry_at)",
    "CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_account_started ON login_sessions(account_id, started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_started ON login_sessions(started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_activity_account_at ON activity_logs(account_id, at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_activity_at ON activity_logs(at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_crm_ops_status_retry ON crm_pending_operations(status, next_retry_at)",
];
