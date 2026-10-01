//! ユーザー自己サービス画面
//!
//! - /my/profile  : display_name / company 自己編集
//! - /my/activity : 自己ログイン履歴・操作履歴 (直近30日)
//!
//! W8 (2026-09-29): 同じデータの JSON 版 `/api/my/{profile,activity}` を `json.rs` に追加
//! (React 画面 `/app/my` 用)。応答型は `data.rs`。

#[cfg(test)]
mod contract_tests;
pub mod data;
mod handlers;
mod json;
mod render;
#[cfg(test)]
mod snapshot_tests;

pub use data::{MyActivityResponse, MyProfileResponse, MyProfileUpdateRequest};
pub use handlers::{my_activity, my_profile_get, my_profile_post};
pub use json::{api_activity, api_profile, api_profile_post};
