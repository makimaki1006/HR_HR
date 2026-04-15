//! ユーザー自己サービス画面
//!
//! - /my/profile  : display_name / company 自己編集
//! - /my/activity : 自己ログイン履歴・操作履歴 (直近30日)

mod handlers;
mod render;

pub use handlers::{my_activity, my_profile_get, my_profile_post};
