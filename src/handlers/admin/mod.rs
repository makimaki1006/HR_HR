//! 管理者画面モジュール
//!
//! - /admin/users           : アカウント一覧
//! - /admin/users/{id}      : 顧客詳細 (プロフィール + ログイン履歴 + 操作履歴)
//! - /admin/login-failures  : ログイン失敗監視

mod handlers;
mod render;

pub use handlers::{admin_login_failures, admin_user_detail, admin_users_list};
