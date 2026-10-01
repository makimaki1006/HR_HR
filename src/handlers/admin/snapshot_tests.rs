//! 管理画面 HTML の snapshot テスト (W8 React 化、計画 §2.8 手順 3) と、
//! React (Vitest) が読む JSON fixture の固定。
//!
//! - `*.html`: ハンドラを「struct を作る → render」に分ける **前** のコードで生成した HTML。
//!   分けた後も 1 バイトも変わっていないことを見る。
//! - `*.json`: 同じ fixture から作った応答 struct の serde_json 出力。`frontend/src/screens/*/`
//!   の Vitest はこのファイルを `?raw` で読み、React の表示値と突き合わせる
//!   (fixture を想像で作らないため)。
//!
//! 作り直すとき: `W8_WRITE_SNAPSHOTS=html` (HTML だけ) / `=json` (JSON だけ) / `=all`。
//! 改行は CRLF/LF を LF にそろえて比べる (Windows の autocrlf 対策)。

use super::data::{self, AdminLoginFailuresResponse, AdminUsersResponse};
use super::render;
use crate::audit::test_fixtures as fx;

/// KPI (直近 30 日) の境界。fixture は 2099 年 (内側) と 2000 年 (外側) なので、
/// この値でも `Utc::now() - 30 日` でも結果は同じ。
pub(crate) const FIXED_CUTOFF: &str = "2050-01-01T00:00:00Z";

fn fixture_path(file: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/w8_admin_my")
        .join(file)
}

fn write_mode(kind: &str) -> bool {
    matches!(
        std::env::var("W8_WRITE_SNAPSHOTS").as_deref(),
        Ok("all") | Ok("1")
    ) || std::env::var("W8_WRITE_SNAPSHOTS").as_deref() == Ok(kind)
}

fn check_file(file: &str, kind: &str, content: &str) {
    let path = fixture_path(file);
    let actual = content.replace("\r\n", "\n");
    if write_mode(kind) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("snapshot {} を読めない: {e}", path.display()))
        .replace("\r\n", "\n");
    assert!(
        actual == expected,
        "{file}: 内容が snapshot と異なる (先頭差分位置 {})\n--- actual ---\n{actual}",
        actual
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(actual.len().min(expected.len()))
    );
}

pub(crate) fn check_snapshot(name: &str, html: &str) {
    check_file(&format!("{name}.html"), "html", html);
}

pub(crate) fn check_json<T: serde::Serialize>(name: &str, value: &T) {
    let json = serde_json::to_string_pretty(value).unwrap() + "\n";
    check_file(&format!("{name}.json"), "json", &json);
}

#[test]
fn admin_users_list_snapshot() {
    let resp = AdminUsersResponse {
        accounts: fx::accounts(),
    };
    check_snapshot("admin_users_list", &render::users_list_page(&resp));
    check_json("admin_users", &resp);
}

#[test]
fn admin_user_detail_snapshot() {
    let resp = data::user_detail(
        fx::hanako(),
        fx::hanako_sessions(),
        fx::hanako_activities(),
        FIXED_CUTOFF,
    );
    check_snapshot("admin_user_detail", &render::user_detail_page(&resp));
    check_json("admin_user_detail", &resp);
}

#[test]
fn admin_login_failures_snapshot() {
    let resp = AdminLoginFailuresResponse {
        failures: fx::login_failures(),
    };
    check_snapshot("admin_login_failures", &render::login_failures_page(&resp));
    check_json("admin_login_failures", &resp);
}

#[test]
fn admin_usage_snapshot() {
    let (by_event, by_account, cross) = fx::usage_rows();
    let resp = data::usage_response(30, by_event, by_account, cross);
    check_snapshot("admin_usage_30d", &render::usage_page(&resp));
    check_json("admin_usage_30d", &resp);
    let empty = data::usage_response(7, vec![], vec![], vec![]);
    check_snapshot("admin_usage_7d_empty", &render::usage_page(&empty));
    check_json("admin_usage_7d_empty", &empty);
}

#[test]
fn admin_static_pages_snapshot() {
    check_snapshot("admin_no_audit_db", &render::no_audit_db());
    check_snapshot("admin_not_found", &render::not_found("acc-<missing>&"));
}

/// JSON fixture の値が HTML snapshot にも出ている (React が読む値 = 旧画面の値)。
#[test]
fn json_fixture_values_appear_in_html_snapshot() {
    let users = AdminUsersResponse {
        accounts: fx::accounts(),
    };
    let html = render::users_list_page(&users);
    for a in &users.accounts {
        assert!(html.contains(&format!("href=\"/admin/users/{}\"", a.id)));
        assert!(html.contains(&format!(">{}<", a.login_count)));
        assert!(html.contains(&a.last_login_at));
    }
    assert!(
        html.contains("F&amp;A &lt;Consulting&gt;"),
        "escape された会社名"
    );
    assert!(html.contains("ユーザー一覧 (3 件)"));

    let detail = data::user_detail(
        fx::hanako(),
        fx::hanako_sessions(),
        fx::hanako_activities(),
        FIXED_CUTOFF,
    );
    let html = render::user_detail_page(&detail);
    assert_eq!(
        detail.kpi_30d,
        data::AdminUserKpi30d {
            login_ok: 1,
            login_fail: 1,
            activity: 3,
            company_views: 2
        }
    );
    assert!(html.contains(r#"<div class="text-3xl font-bold">1</div>"#));
    assert!(html.contains(r#"<div class="text-3xl font-bold text-red-400">1</div>"#));
    assert!(html.contains(r#"<div class="text-3xl font-bold">3</div>"#));
    assert!(html.contains(r#"<div class="text-3xl font-bold">2</div>"#));
    let ua40: String = fx::LONG_UA.chars().take(40).collect();
    assert!(html.contains(&ua40));
    assert!(!html.contains(fx::LONG_UA), "HTML は 40 文字に切り詰める");
    assert_eq!(detail.sessions[0].user_agent, fx::LONG_UA, "JSON は全文");
}
