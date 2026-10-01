pub mod admin;
pub mod analysis;
pub mod api;
pub mod api_v1;
pub mod app_api; // 2026-09-29: React 画面用の JSON API (/api/app/*、型は ts-rs で生成)
pub mod balance;
pub mod call_quality;
pub mod company;
pub mod comparison;
pub mod competitive;
pub mod competitor;
#[cfg(test)]
mod competitor_tests;
pub mod consult;
pub mod cs_dashboard; // 2026-09-21: コンサルKPI。上の consult（商談準備レポート）とは別物
pub mod demographics;
pub mod diagnostic;
pub mod dict_cards;
pub mod driver;
pub mod emp_classifier;
pub mod filters; // 2026-09-30: ヘッダーフィルタの読み出し (/api/filters/current) と resolve_filters
pub mod geo_api; // 2026-10-01: 都道府県・市区町村の JSON 版 (/api/app/geo/*)。HTML 版 (api.rs) と取得関数を共有
pub mod guide;
pub mod helpers;
pub mod indeed;
pub mod insight;
pub mod integrated_report;
pub mod jobmap;
pub mod license;
pub mod market;
pub mod my;
pub mod nav; // 2026-09-30: ナビ定義 (旧シェルと React シェル共通、/api/nav、hidden フラグ)
pub mod overview;
pub mod recruitment_diag;
pub mod region;
pub mod regional_analysis;
pub mod sales_kpi; // 2026-09-05: 営業KPI（現場版）
pub mod spa_shell; // 2026-09-29: React 画面の HTML シェル (/app/{screen})
pub mod survey;
pub mod trend;
pub mod types;
pub mod workstyle;

// Team δ 監査 (2026-04-23): 全タブ Frontend⇔Backend JSON 契約 L5 逆証明
// （採用診断以外の jobmap 主要 endpoint + 既知ミスマッチの記録テスト）
#[cfg(test)]
mod global_contract_audit_test;

// Tab 深掘り (2026-04-26): 媒体分析以外の 7 タブ 因果断定文言 逆証明テスト
// （feedback_correlation_not_causation.md / feedback_reverse_proof_tests.md 準拠）
#[cfg(test)]
mod tab_phrase_audit_test;
