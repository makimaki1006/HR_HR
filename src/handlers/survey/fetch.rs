//! DB queries for survey module
//! 期限切れセッション削除等

type Db = crate::db::local_sqlite::LocalDb;

/// 起動時に期限切れセッションを削除
pub fn cleanup_expired_sessions(db: &Db) {
    let sqls = [
        "DELETE FROM survey_records WHERE session_id IN (SELECT id FROM survey_sessions WHERE expires_at < datetime('now'))",
        "DELETE FROM survey_sessions WHERE expires_at < datetime('now')",
    ];
    for sql in &sqls {
        if let Err(e) = db.execute(sql, &[]) {
            tracing::debug!("Survey cleanup (expected if tables don't exist): {e}");
        }
    }
}

/// surveyテーブル作成（存在しない場合）
pub fn ensure_survey_tables(db: &Db) {
    let sqls = [
        "CREATE TABLE IF NOT EXISTS survey_sessions (
            id TEXT PRIMARY KEY,
            user_email TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL DEFAULT 'unknown',
            filename TEXT,
            record_count INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now')),
            expires_at TEXT DEFAULT (datetime('now', '+7 days'))
        )",
        "CREATE TABLE IF NOT EXISTS survey_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            row_index INTEGER,
            job_title TEXT,
            company_name TEXT,
            location_raw TEXT,
            salary_raw TEXT,
            employment_type TEXT,
            tags_raw TEXT,
            url TEXT,
            is_new INTEGER DEFAULT 0,
            parsed_prefecture TEXT,
            parsed_municipality TEXT,
            parsed_region_block TEXT,
            salary_type TEXT,
            salary_min INTEGER,
            salary_max INTEGER,
            unified_monthly INTEGER,
            salary_confidence REAL,
            location_confidence REAL
        )",
        "CREATE INDEX IF NOT EXISTS idx_survey_session ON survey_records(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_survey_pref ON survey_records(parsed_prefecture)",
    ];
    for sql in &sqls {
        if let Err(e) = db.execute(sql, &[]) {
            tracing::warn!("Survey table creation failed: {e}");
        }
    }
}
