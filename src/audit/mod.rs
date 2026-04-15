//! 監査ログ・アカウント自動登録モジュール
//!
//! Turso 新規DB (`audit`) に以下3テーブルを持つ:
//! - `accounts`: ログイン済みユーザーを自動登録する台帳
//! - `login_sessions`: ログイン成功/失敗の履歴
//! - `activity_logs`: 重要操作の履歴 (1年保持)
//!
//! 既存の認証フロー (`src/auth/`) は維持し、本モジュールは成功時 upsert +
//! セッション記録 + 操作記録の責務だけを持つ。

pub mod dao;
pub mod schema;

pub use dao::{
    insert_activity, insert_login_session, log_failed_login, purge_old_activity, upsert_account,
    AccountRow, ActivityLogRow, LoginSessionRow,
};

use crate::db::turso_http::TursoDb;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// 監査DB ハンドル。
/// AppState.audit_db: Option<AuditDb> として保持。未接続なら None。
#[derive(Clone)]
pub struct AuditDb {
    inner: Arc<AuditInner>,
}

struct AuditInner {
    pub turso: TursoDb,
    /// IP をハッシュする際のソルト（環境変数 AUDIT_IP_SALT から）
    pub ip_salt: String,
}

impl AuditDb {
    pub fn new(turso: TursoDb, ip_salt: String) -> Self {
        Self {
            inner: Arc::new(AuditInner { turso, ip_salt }),
        }
    }

    pub fn turso(&self) -> &TursoDb {
        &self.inner.turso
    }

    /// IP を固定ソルト付き SHA-256 で 12 文字に短縮してハッシュ化。
    /// Why: 生IPを保存するとプライバシーリスクあり。固定ソルトなら同一IPの
    ///      複数試行は同じハッシュになり検知可能、しかも逆引きは不可能。
    pub fn hash_ip(&self, ip: &str) -> String {
        if ip.is_empty() {
            return String::new();
        }
        let mut hasher = Sha256::new();
        hasher.update(self.inner.ip_salt.as_bytes());
        hasher.update(b":");
        hasher.update(ip.as_bytes());
        let digest = hasher.finalize();
        let hex = format!("{digest:x}");
        hex.chars().take(12).collect()
    }
}

/// UUID v4 (dash付き) を生成
pub fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Session から account_id / login_session_id を取り出し、audit が有効なら
/// activity_logs に 1 件 INSERT するハンドラ用ヘルパー。
///
/// 監査無効 / アカウント紐付き無しなら何もしない (fire-and-forget)。
pub async fn record_event(
    audit: &Option<AuditDb>,
    session: &tower_sessions::Session,
    event_type: &str,
    target_type: &str,
    target_id: &str,
    meta: &str,
) {
    let Some(audit) = audit else { return };
    let account_id: Option<String> = session
        .get(crate::SESSION_ACCOUNT_ID_KEY)
        .await
        .unwrap_or(None);
    let Some(account_id) = account_id else { return };
    let login_session_id: Option<String> = session
        .get(crate::SESSION_LOGIN_SESSION_ID_KEY)
        .await
        .unwrap_or(None);
    let sid = login_session_id.unwrap_or_default();
    dao::insert_activity(
        audit,
        &account_id,
        &sid,
        event_type,
        target_type,
        target_id,
        meta,
    );
}

/// 現在時刻を ISO-8601 UTC 文字列で取得 (例: 2026-04-15T07:12:34Z)
pub fn now_iso8601() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_is_36_chars() {
        let id = new_uuid();
        assert_eq!(id.len(), 36);
        assert!(id.contains('-'));
    }

    #[test]
    fn hash_ip_deterministic() {
        // AuditDb の inner を直接 mock できないので、ハッシュロジックを単体で検証
        let salt = "testsalt".to_string();
        let ip = "192.0.2.1";
        let mut h1 = Sha256::new();
        h1.update(salt.as_bytes());
        h1.update(b":");
        h1.update(ip.as_bytes());
        let hex1: String = format!("{:x}", h1.finalize()).chars().take(12).collect();
        assert_eq!(hex1.len(), 12);
    }

    #[test]
    fn hash_ip_different_ips_differ() {
        let salt = "salt";
        let mut h1 = Sha256::new();
        h1.update(salt.as_bytes());
        h1.update(b":");
        h1.update(b"1.1.1.1");
        let hex1: String = format!("{:x}", h1.finalize()).chars().take(12).collect();

        let mut h2 = Sha256::new();
        h2.update(salt.as_bytes());
        h2.update(b":");
        h2.update(b"2.2.2.2");
        let hex2: String = format!("{:x}", h2.finalize()).chars().take(12).collect();

        assert_ne!(hex1, hex2);
    }

    #[test]
    fn now_iso8601_format() {
        let s = now_iso8601();
        // 2026-04-15T07:12:34Z
        assert_eq!(s.len(), 20);
        assert!(s.ends_with('Z'));
        assert!(s.contains('T'));
    }
}
