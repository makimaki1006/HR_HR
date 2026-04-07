mod fetch;
mod handlers;
pub mod region;
mod render;
mod stats;
pub mod company_markers;

pub use handlers::{
    tab_jobmap, jobmap_markers, jobmap_detail, jobmap_detail_json, jobmap_stats, jobmap_municipalities,
    jobmap_seekers, jobmap_seeker_detail, jobmap_choropleth,
};
pub use region::{region_summary, region_age_gender, region_posting_stats, region_segments};
pub use company_markers::company_markers as jobmap_company_markers;
pub use company_markers::labor_flow as jobmap_labor_flow;
