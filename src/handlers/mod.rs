pub mod admin;
pub mod analysis;
pub mod api;
pub mod api_v1;
pub mod balance;
pub mod company;
pub mod competitive;
pub mod demographics;
pub mod diagnostic;
pub mod guide;
pub mod helpers;
pub mod insight;
pub mod jobmap;
pub mod market;
pub mod my;
pub mod overview;
pub mod recruitment_diag;
pub mod region;
pub mod survey;
pub mod trend;
pub mod workstyle;

// Team δ 監査 (2026-04-23): 全タブ Frontend⇔Backend JSON 契約 L5 逆証明
// （採用診断以外の jobmap 主要 endpoint + 既知ミスマッチの記録テスト）
#[cfg(test)]
mod global_contract_audit_test;
