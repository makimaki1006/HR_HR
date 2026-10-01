//! 監査 DB の行のテスト用 fixture (W8: admin / my の snapshot テストと contract テストで共用)。
//!
//! 値は「DAO が返す順序」(started_at / at / last_login_at の降順) で並べてある。
//! contract テストはこの行をそのまま SQLite に入れ、DAO 経由の JSON がこれと一致することを見る。
//! 日時は `RECENT` (2099 年 = 常に「直近 30 日」の内側) と `OLD` (2000 年 = 常に外側) を使い、
//! `chrono::Utc::now()` に依存する KPI が実行日に左右されないようにしている。

use super::dao::{AccountRow, ActivityLogRow, LoginSessionRow, UsageRow};

/// 常に「直近 30 日」の内側になる日付の接頭辞
pub const RECENT_DAY: &str = "2099-01-02";
/// 常に「直近 30 日」の外側になる日時
pub const OLD_AT: &str = "2000-01-01T00:00:00Z";

pub const HANAKO_ID: &str = "acc-0001";
pub const JIRO_ID: &str = "acc-0002";
pub const SABURO_ID: &str = "acc-0003";

/// 40 文字 (admin) / 50 文字 (my) の切り詰めが効くように 40 文字より長い User-Agent
pub const LONG_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130";

/// accounts (last_login_at 降順)。会社名に `<` `&` を入れ、エスケープが snapshot に写るようにしている。
pub fn accounts() -> Vec<AccountRow> {
    vec![
        AccountRow {
            id: HANAKO_ID.into(),
            email: "hanako@f-a-c.co.jp".into(),
            display_name: "山田 花子".into(),
            company: "F&A <Consulting>".into(),
            role: "user".into(),
            first_seen_at: "2026-01-05T09:00:00Z".into(),
            last_login_at: "2026-09-28T01:23:45Z".into(),
            login_count: 12,
            disabled_at: String::new(),
        },
        AccountRow {
            id: JIRO_ID.into(),
            email: "jiro@client.example".into(),
            display_name: String::new(),
            company: String::new(),
            role: "user".into(),
            first_seen_at: "2026-03-01T00:00:00Z".into(),
            last_login_at: "2026-09-20T10:00:00Z".into(),
            login_count: 3,
            disabled_at: String::new(),
        },
        AccountRow {
            id: SABURO_ID.into(),
            email: "saburo@f-a-c.co.jp".into(),
            display_name: "佐藤 三郎".into(),
            company: "株式会社サンプル".into(),
            role: "admin".into(),
            first_seen_at: "2025-12-01T00:00:00Z".into(),
            last_login_at: "2026-08-15T12:00:00Z".into(),
            login_count: 45,
            disabled_at: "2026-09-01T00:00:00Z".into(),
        },
    ]
}

pub fn hanako() -> AccountRow {
    accounts().remove(0)
}

/// 花子の login_sessions (started_at 降順)。KPI 期待値: 直近 30 日 成功 1 / 失敗 1。
pub fn hanako_sessions() -> Vec<LoginSessionRow> {
    vec![
        LoginSessionRow {
            id: "ses-0001".into(),
            account_id: HANAKO_ID.into(),
            attempted_email: String::new(),
            started_at: format!("{RECENT_DAY}T09:00:00Z"),
            ended_at: format!("{RECENT_DAY}T10:00:00Z"),
            ip_hash: "ab12cd34ef56".into(),
            user_agent: LONG_UA.into(),
            login_method: "password_internal".into(),
            success: 1,
            failure_reason: String::new(),
        },
        LoginSessionRow {
            id: "ses-0002".into(),
            account_id: HANAKO_ID.into(),
            attempted_email: "hanako@f-a-c.co.jp".into(),
            started_at: format!("{RECENT_DAY}T08:00:00Z"),
            ended_at: String::new(),
            ip_hash: "ab12cd34ef56".into(),
            user_agent: "curl/8.0".into(),
            login_method: "password".into(),
            success: 0,
            failure_reason: "wrong_password".into(),
        },
        LoginSessionRow {
            id: "ses-0003".into(),
            account_id: HANAKO_ID.into(),
            attempted_email: String::new(),
            started_at: OLD_AT.into(),
            ended_at: String::new(),
            ip_hash: "0000aaaa1111".into(),
            user_agent: "Safari/17".into(),
            login_method: "google_oidc".into(),
            success: 1,
            failure_reason: String::new(),
        },
    ]
}

/// 花子の activity_logs (at 降順)。KPI 期待値: 直近 30 日 操作 3 / 企業閲覧 2。
pub fn hanako_activities() -> Vec<ActivityLogRow> {
    vec![
        ActivityLogRow {
            id: "act-0001".into(),
            account_id: HANAKO_ID.into(),
            session_id: "ses-0001".into(),
            at: format!("{RECENT_DAY}T09:30:00Z"),
            event_type: "view_company_profile".into(),
            target_type: "company".into(),
            target_id: "1234567890123".into(),
            meta: String::new(),
        },
        ActivityLogRow {
            id: "act-0002".into(),
            account_id: HANAKO_ID.into(),
            session_id: "ses-0001".into(),
            at: format!("{RECENT_DAY}T09:20:00Z"),
            event_type: "view_tab".into(),
            target_type: "tab".into(),
            target_id: "/tab/survey".into(),
            meta: String::new(),
        },
        ActivityLogRow {
            id: "act-0003".into(),
            account_id: HANAKO_ID.into(),
            session_id: "ses-0001".into(),
            at: format!("{RECENT_DAY}T09:10:00Z"),
            event_type: "view_company_profile".into(),
            target_type: "company".into(),
            target_id: "9876543210987".into(),
            meta: String::new(),
        },
        ActivityLogRow {
            id: "act-0004".into(),
            account_id: HANAKO_ID.into(),
            session_id: "ses-0003".into(),
            at: OLD_AT.into(),
            event_type: "login".into(),
            target_type: String::new(),
            target_id: String::new(),
            meta: String::new(),
        },
    ]
}

/// 全アカウント横断のログイン失敗 (started_at 降順)。account_id 無し = 未登録メールの試行。
pub fn login_failures() -> Vec<LoginSessionRow> {
    vec![
        LoginSessionRow {
            id: "fail-0001".into(),
            account_id: String::new(),
            attempted_email: "attacker@evil.example".into(),
            started_at: "2026-09-28T00:00:10Z".into(),
            ended_at: String::new(),
            ip_hash: "ff00ff00ff00".into(),
            user_agent: "python-requests/2.32".into(),
            login_method: "password".into(),
            success: 0,
            failure_reason: "invalid_domain".into(),
        },
        LoginSessionRow {
            id: "fail-0002".into(),
            account_id: String::new(),
            attempted_email: "hanako@f-a-c.co.jp".into(),
            started_at: "2026-09-28T00:00:05Z".into(),
            ended_at: String::new(),
            ip_hash: "ab12cd34ef56".into(),
            user_agent: LONG_UA.into(),
            login_method: "password".into(),
            success: 0,
            failure_reason: "wrong_password".into(),
        },
    ]
}

fn usage_row(account_id: &str, email: &str, ev: &str, target: &str, cnt: i64) -> UsageRow {
    UsageRow {
        account_id: account_id.into(),
        email: email.into(),
        event_type: ev.into(),
        target_id: target.into(),
        count: cnt,
        last_at: "2026-08-10T09:00:00Z".into(),
    }
}

/// 利用状況の集計行 (機能別 / ユーザー別 / ユーザー×機能)。
pub fn usage_rows() -> (Vec<UsageRow>, Vec<UsageRow>, Vec<UsageRow>) {
    let by_event = vec![
        usage_row("", "", "view_tab", "/tab/survey", 42),
        usage_row("", "", "upload_survey_csv", "", 7),
        usage_row("", "", "mystery_event", "", 1),
    ];
    let by_account = vec![
        usage_row(HANAKO_ID, "hanako@f-a-c.co.jp", "", "", 49),
        usage_row("acc-gone", "", "", "", 1),
    ];
    let cross = vec![
        usage_row(
            HANAKO_ID,
            "hanako@f-a-c.co.jp",
            "view_tab",
            "/tab/survey",
            42,
        ),
        usage_row(HANAKO_ID, "hanako@f-a-c.co.jp", "upload_survey_csv", "", 7),
        usage_row("acc-gone", "", "mystery_event", "", 1),
    ];
    (by_event, by_account, cross)
}
