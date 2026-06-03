mod analysis;
mod external;
mod fetch;
mod handlers;
mod render;
mod utils;

#[cfg(test)]
mod tests;

// ハンドラ（lib.rsから handlers::competitive::* として参照）
pub use handlers::{
    comp_analysis, comp_analysis_filtered, comp_facility_types, comp_filter, comp_municipalities,
    comp_report, comp_service_types, tab_competitive,
};

// 外部統計ドリルダウン (10 endpoint)
pub use external::{
    ext_daytime_population, ext_education, ext_household_spending, ext_households,
    ext_industry_employees, ext_job_ratio, ext_labor_force, ext_min_wage, ext_social_life,
    ext_turnover,
};

// 他モジュールから参照されるユーティリティ
pub use utils::escape_html;
pub use utils::truncate_str;
pub use utils::{build_option, build_option_with_data};
