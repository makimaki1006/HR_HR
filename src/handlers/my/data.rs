//! 個人設定画面の応答型と、監査 DB からそれを組み立てる関数 (W8 React 化、計画 §2.8 手順 1-3)。
//!
//! HTML (`handlers.rs` → `render.rs`) と JSON (`json.rs`、`/api/my/*`) が同じ型を使う。
//! 3 状態 (`ok` / `audit_disabled` / `not_linked`) は旧画面が別ページとして描いていたので、
//! `status` タグ付きの enum にして React 側でも同じ文言を出せるようにしている。

use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use ts_rs::TS;

use crate::audit::dao::{self, AccountRow, ActivityLogRow, LoginSessionRow};
use crate::AppState;

/// `GET /api/my/profile` / `POST /api/my/profile` の応答 (旧 `/my/profile`)。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MyProfileResponse {
    Ok {
        account: AccountRow,
    },
    /// 監査 DB 未接続 (AUDIT_TURSO_URL 未設定)。旧画面: 「この機能は現在ご利用いただけません」
    AuditDisabled,
    /// セッションに account_id が無い / accounts に行が無い。旧画面: 「アカウントが見つかりません」
    NotLinked,
}

/// `GET /api/my/activity` の応答 (旧 `/my/activity`)。ログイン 50 件 / 操作 100 件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MyActivityResponse {
    Ok {
        account: AccountRow,
        sessions: Vec<LoginSessionRow>,
        activities: Vec<ActivityLogRow>,
    },
    AuditDisabled,
    NotLinked,
}

/// プロフィール更新の入力。HTML フォーム (`POST /my/profile`) と JSON (`POST /api/my/profile`) で共通。
#[derive(Debug, Clone, Default, Deserialize, TS)]
pub struct MyProfileUpdateRequest {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub company: String,
}

/// 氏名の最大文字数 (表示崩れ防止)。React の `maxlength` と同じ値。
pub const DISPLAY_NAME_MAX_CHARS: usize = 80;
/// 会社名の最大文字数。
pub const COMPANY_MAX_CHARS: usize = 120;

/// プロフィール更新の結果。HTML と JSON で応答の形が違うので、決定だけを返す。
#[derive(Debug)]
pub enum ProfileUpdateOutcome {
    /// 監査 DB 未接続
    AuditDisabled,
    /// ログイン済みだがセッションに account_id が無い (旧 HTML は /login へ 303)
    NoAccountInSession,
    /// 更新を実行した。再取得したアカウント (取れなければ None = not_linked)
    Updated(Option<AccountRow>),
}

async fn current_account_id(session: &Session) -> Option<String> {
    session
        .get(crate::SESSION_ACCOUNT_ID_KEY)
        .await
        .unwrap_or(None)
}

async fn blocking<T: Send + 'static>(
    what: &'static str,
    f: impl FnOnce() -> T + Send + 'static,
    fallback: impl FnOnce() -> T,
) -> T {
    match tokio::task::spawn_blocking(f).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("{what} spawn_blocking join failed: {e}");
            fallback()
        }
    }
}

/// GET /my/profile の中身
pub async fn load_profile(state: &AppState, session: &Session) -> MyProfileResponse {
    let Some(audit) = &state.audit else {
        return MyProfileResponse::AuditDisabled;
    };
    let Some(aid) = current_account_id(session).await else {
        return MyProfileResponse::NotLinked;
    };
    let audit = audit.clone();
    let acc = blocking(
        "my_profile_get",
        move || dao::find_account_by_id(audit.turso(), &aid),
        || None,
    )
    .await;
    match acc {
        Some(account) => MyProfileResponse::Ok { account },
        None => MyProfileResponse::NotLinked,
    }
}

/// GET /my/activity の中身
pub async fn load_activity(state: &AppState, session: &Session) -> MyActivityResponse {
    let Some(audit) = &state.audit else {
        return MyActivityResponse::AuditDisabled;
    };
    let Some(aid) = current_account_id(session).await else {
        return MyActivityResponse::NotLinked;
    };
    let audit = audit.clone();
    // 3 つの blocking DAO 呼出を 1 度の spawn_blocking にまとめる
    let (acc, sessions, activities) = blocking(
        "my_activity",
        move || {
            let acc = dao::find_account_by_id(audit.turso(), &aid);
            let sessions = dao::list_sessions_for_account(audit.turso(), &aid, 50);
            let activities = dao::list_activity_for_account(audit.turso(), &aid, 100);
            (acc, sessions, activities)
        },
        || (None, Vec::new(), Vec::new()),
    )
    .await;
    match acc {
        Some(account) => MyActivityResponse::Ok {
            account,
            sessions,
            activities,
        },
        None => MyActivityResponse::NotLinked,
    }
}

/// POST /my/profile と POST /api/my/profile の共通の書き込み経路
/// (長さ制限 → `dao::update_profile` → 監査記録 `update_profile` → 再取得)。
pub async fn apply_profile_update(
    state: &AppState,
    session: &Session,
    req: &MyProfileUpdateRequest,
) -> ProfileUpdateOutcome {
    let Some(audit) = &state.audit else {
        return ProfileUpdateOutcome::AuditDisabled;
    };
    let Some(aid) = current_account_id(session).await else {
        return ProfileUpdateOutcome::NoAccountInSession;
    };
    // 長さ制限（表示崩れ防止）
    let name = req
        .display_name
        .chars()
        .take(DISPLAY_NAME_MAX_CHARS)
        .collect::<String>();
    let company = req
        .company
        .chars()
        .take(COMPANY_MAX_CHARS)
        .collect::<String>();
    {
        let audit = audit.clone();
        let aid = aid.clone();
        match tokio::task::spawn_blocking(move || {
            dao::update_profile(audit.turso(), &aid, &name, &company)
        })
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("update_profile failed: {e}"),
            Err(e) => tracing::warn!("update_profile spawn_blocking join failed: {e}"),
        }
    }

    // 監査: プロフィール更新を記録
    crate::audit::record_event(&state.audit, session, "update_profile", "account", &aid, "").await;

    // 更新後のプロフィール取得して再表示
    let audit = audit.clone();
    let acc = blocking(
        "my_profile_post find_account",
        move || dao::find_account_by_id(audit.turso(), &aid),
        || None,
    )
    .await;
    ProfileUpdateOutcome::Updated(acc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::test_fixtures as fx;

    /// React が `status` で分岐できる形 (内部タグ付き enum) で出ること
    #[test]
    fn responses_are_tagged_by_status() {
        let v = serde_json::to_value(MyProfileResponse::AuditDisabled).unwrap();
        assert_eq!(v, serde_json::json!({"status": "audit_disabled"}));
        let v = serde_json::to_value(MyProfileResponse::NotLinked).unwrap();
        assert_eq!(v, serde_json::json!({"status": "not_linked"}));
        let v = serde_json::to_value(MyProfileResponse::Ok {
            account: fx::hanako(),
        })
        .unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["account"]["email"], "hanako@f-a-c.co.jp");
        assert_eq!(v["account"]["login_count"], 12);
        let v = serde_json::to_value(MyActivityResponse::Ok {
            account: fx::hanako(),
            sessions: fx::hanako_sessions(),
            activities: fx::hanako_activities(),
        })
        .unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["sessions"].as_array().unwrap().len(), 3);
        assert_eq!(v["activities"][0]["target_id"], "1234567890123");
        let v = serde_json::to_value(MyActivityResponse::AuditDisabled).unwrap();
        assert_eq!(v, serde_json::json!({"status": "audit_disabled"}));
    }

    /// 入力はキー欠落でも空文字で受ける (フォーム版は contract_tests の旧フォーム POST で確認)
    #[test]
    fn update_request_defaults_missing_fields() {
        let r: MyProfileUpdateRequest = serde_json::from_str(r#"{"company":"X"}"#).unwrap();
        assert_eq!(r.display_name, "");
        assert_eq!(r.company, "X");
        let r: MyProfileUpdateRequest = serde_json::from_str("{}").unwrap();
        assert_eq!((r.display_name.as_str(), r.company.as_str()), ("", ""));
    }
}
