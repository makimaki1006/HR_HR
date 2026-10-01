//! 個人設定画面 HTML の snapshot テスト (W8 React 化、計画 §2.8 手順 3) と JSON fixture。
//! 仕組みは `handlers::admin::snapshot_tests` と同じ。

use super::data::{MyActivityResponse, MyProfileResponse};
use super::render;
use crate::audit::test_fixtures as fx;
use crate::handlers::admin::snapshot_tests::{check_json, check_snapshot};

#[test]
fn my_profile_snapshot() {
    let resp = MyProfileResponse::Ok {
        account: fx::hanako(),
    };
    check_snapshot("my_profile", &render::profile_page(&resp, None));
    check_snapshot(
        "my_profile_flash",
        &render::profile_page(&resp, Some("プロフィールを更新しました")),
    );
    check_json("my_profile", &resp);
    check_json(
        "my_profile_audit_disabled",
        &MyProfileResponse::AuditDisabled,
    );
    check_json("my_profile_not_linked", &MyProfileResponse::NotLinked);
}

#[test]
fn my_activity_snapshot() {
    let resp = MyActivityResponse::Ok {
        account: fx::hanako(),
        sessions: fx::hanako_sessions(),
        activities: fx::hanako_activities(),
    };
    check_snapshot("my_activity", &render::activity_page(&resp));
    check_json("my_activity", &resp);
}

#[test]
fn my_static_pages_snapshot() {
    check_snapshot("my_audit_disabled", &render::audit_disabled_page());
    check_snapshot("my_not_linked", &render::not_linked_page());
    // status 付きの応答からも同じページになる (旧ハンドラの分岐を render に移した)
    assert_eq!(
        render::profile_page(&MyProfileResponse::AuditDisabled, None),
        render::audit_disabled_page()
    );
    assert_eq!(
        render::profile_page(&MyProfileResponse::NotLinked, Some("x")),
        render::not_linked_page()
    );
    assert_eq!(
        render::activity_page(&MyActivityResponse::AuditDisabled),
        render::audit_disabled_page()
    );
    assert_eq!(
        render::activity_page(&MyActivityResponse::NotLinked),
        render::not_linked_page()
    );
}
