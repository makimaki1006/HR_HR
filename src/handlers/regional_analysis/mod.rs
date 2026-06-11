//! 地域分析タブ。
//!
//! 外部統計 (e-Stat / 国勢調査 / 公的統計) のみで構成される常設タブ。
//! postings (HW 掲載求人) への依存なし。

mod fetch;
mod handlers;
mod render;

// lib.rs から handlers::regional_analysis::* として参照
pub use handlers::{
    regional_company_matrix, regional_foreign_residents, regional_industry_structure,
    regional_internet_usage, regional_job_openings_ratio, regional_labor_stats,
    regional_municipalities, regional_occupation, regional_population_pyramid,
    regional_wage_comparison, tab_regional_analysis,
};
