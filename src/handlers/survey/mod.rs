//! 競合調査モジュール（GAS移植）
//! Indeed/求人ボックスCSVをアップロードし、HWデータ・外部統計と統合して
//! 地域特化型の競合調査レポートを生成する。

pub mod aggregator;
pub mod fetch;
pub mod handlers;
pub mod integration;
pub mod job_seeker;
pub mod location_parser;
pub mod render;
pub mod report;
pub mod report_html;
pub mod salary_parser;
pub mod statistics;
pub mod upload;

#[cfg(test)]
mod location_parser_realdata_test;

#[cfg(test)]
mod parser_aggregator_audit_test;

#[cfg(test)]
mod report_html_qa_test;

pub use handlers::{
    analyze_survey, integrate_report, report_json, survey_report_html, tab_survey, upload_csv,
};
