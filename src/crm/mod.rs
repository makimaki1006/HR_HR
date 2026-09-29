//! Headless CRM の `/api/crm/*` (読み取り)。
//!
//! - `rbac`: 認可 (誰が読めるか)。許可条件は `rbac.rs` 1 箇所
//! - `routes`: `GET /api/crm/{contacts|companies|deals}/{id}`
//!
//! ルートは `lib.rs` の `protected_routes` に merge する (require_auth 配下。未ログインは 303 /login)。
//! 認証不要の `/api/v1/*` には置かない。書き込みはしない。

pub mod rbac;
pub mod routes;

pub use routes::router;

#[cfg(test)]
mod routes_tests;
