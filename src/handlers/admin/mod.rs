//! 管理者画面モジュール
//!
//! - /admin/users           : アカウント一覧
//! - /admin/users/{id}      : 顧客詳細 (プロフィール + ログイン履歴 + 操作履歴)
//! - /admin/login-failures  : ログイン失敗監視
//! - /admin/usage           : 利用状況 (ユーザー別 / 機能別 / クロス集計) 2026-08-10
//!
//! W8 (2026-09-29): 同じデータの JSON 版 `/api/admin/{users,users/{id},login-failures,usage}`
//! を `json.rs` に追加 (React 画面 `/app/admin` 用)。応答 struct は `data.rs`。

pub mod data;
mod handlers;
pub mod hubspot_check;
mod json;
mod render;
// W8 (2026-09-29): struct → render 分割の前後で HTML が変わらないことを固定する
#[cfg(test)]
pub(crate) mod snapshot_tests;
// W8: /api/admin/* の contract テスト (偽 Turso + build_app)。helper は my からも使う
#[cfg(test)]
pub(crate) mod contract_tests;

pub use data::{
    AdminLoginFailuresResponse, AdminUsageEntry, AdminUsageResponse, AdminUserDetailResponse,
    AdminUserKpi30d, AdminUsersResponse,
};
pub use handlers::{admin_login_failures, admin_usage, admin_user_detail, admin_users_list};
pub use hubspot_check::api_hubspot_check;
pub use json::{api_login_failures, api_usage, api_user_detail, api_users};
