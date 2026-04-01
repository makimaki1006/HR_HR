//! 複合示唆エンジン（Insight Engine）
//! 全データソース（ローカルSQLite + Turso時系列 + Turso外部統計）を複合的に
//! 掛け合わせ、採用構造・将来予測・地域比較・アクション提案の4カテゴリで
//! 示唆（インサイト）を生成する。

pub mod engine;
pub mod export;
pub mod fetch;
pub mod handlers;
pub mod helpers;
pub mod render;
pub mod report;

pub use handlers::{tab_insight, insight_subtab, insight_widget, insight_report_json, insight_report_html};
pub use export::insight_report_xlsx;
