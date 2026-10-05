//! Headless CRM の `/api/crm/*` (HubSpot からの読み取りだけ。書き込みはしない)。
//!
//! - `rbac`: 認可 (誰が読めるか)。metadata とレコード読み取りで同じ基準。許可条件は `rbac.rs` 1 箇所
//! - `call_queue`: `GET /api/crm/call-queue` (架電キュー。HubSpot の Deal 検索 + 関連の一括読み取り)
//! - `routes`: `GET /api/crm/metadata` と `GET /api/crm/{contacts|companies|deals}/{id}`
//!
//! ルートは `lib.rs` の `protected_routes` **の外**に merge する (未ログインを /login への 303 でなく
//! JSON の 401 で返すため。認可は各ハンドラの先頭)。認証不要の `/api/v1/*` には置かない。
//! HubSpot への通信は `crate::hubspot::HubSpotClient` (`AppState.hubspot`) に一本化している。

pub mod call_queue;
pub mod rbac;
pub mod routes;

pub use routes::router;

#[cfg(test)]
mod call_queue_tests;
#[cfg(test)]
mod real_hubspot_smoke;
#[cfg(test)]
mod routes_tests;
