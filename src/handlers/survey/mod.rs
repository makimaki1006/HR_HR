//! 競合調査モジュール（GAS移植）
//! Indeed/求人ボックスCSVをアップロードし、HWデータ・外部統計と統合して
//! 地域特化型の競合調査レポートを生成する。

pub mod salary_parser;
pub mod location_parser;
pub mod statistics;
pub mod aggregator;
pub mod job_seeker;
pub mod upload;
pub mod fetch;
pub mod integration;
pub mod handlers;
pub mod render;
pub mod report;
pub mod report_html;

pub use handlers::{tab_survey, upload_csv, analyze_survey, integrate_report, report_json, survey_report_html};
