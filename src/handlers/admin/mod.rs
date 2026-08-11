//! 管理者画面モジュール
//!
//! - /admin/users           : アカウント一覧
//! - /admin/users/{id}      : 顧客詳細 (プロフィール + ログイン履歴 + 操作履歴)
//! - /admin/login-failures  : ログイン失敗監視
//! - /admin/usage           : 利用状況 (ユーザー別 / 機能別 / クロス集計) 2026-08-10

mod handlers;
mod render;

pub use handlers::{admin_login_failures, admin_usage, admin_user_detail, admin_users_list};
