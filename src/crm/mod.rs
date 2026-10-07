//! Headless CRM の `/api/crm/*` (HubSpot からの読み取りだけ。書き込みはしない)。
//!
//! - `rbac`: 認可 (誰が使えるか。会社の Google ログインなら全員が全件を読める。管理者 = `ADMIN_EMAILS` または `accounts.role = admin` はキューの既定が全員分、それ以外は自分)。許可条件は `rbac.rs` 1 箇所
//! - `record_gate`: レコード単位の制限。CRM の利用者には掛けない (役割が決まっていない最小権限の人だけの備え)
//! - `call_queue`: `GET /api/crm/call-queue` (架電キュー。HubSpot の Deal 検索 + 関連の一括読み取り)
//! - `owners`: `GET /api/crm/owners` (CRM の利用者全員。所有者を名前で選ぶための一覧。HubSpot Owners API、10 分キャッシュ)
//! - `workspace`: `GET /api/crm/workspace/deals/{id}` (架電ワークスペースの詳細。案件・担当者・会社・活動履歴)
//! - `routes`: `GET /api/crm/metadata` と `GET /api/crm/{contacts|companies|deals}/{id}`
//!
//! ルートは `lib.rs` の `protected_routes` **の外**に merge する (未ログインを /login への 303 でなく
//! JSON の 401 で返すため。認可は各ハンドラの先頭)。認証不要の `/api/v1/*` には置かない。
//! HubSpot への通信は `crate::hubspot::HubSpotClient` (`AppState.hubspot`) に一本化している。

pub mod call_queue;
pub mod owners;
pub mod rbac;
pub mod record_gate;
pub mod routes;
pub mod workspace;

pub use routes::router;

#[cfg(test)]
mod call_queue_tests;
#[cfg(test)]
mod real_hubspot_smoke;
#[cfg(test)]
mod roles_tests;
#[cfg(test)]
mod routes_tests;
#[cfg(test)]
mod workspace_tests;
