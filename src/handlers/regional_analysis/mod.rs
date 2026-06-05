//! 地域×業界分析タブ (Phase1)。
//!
//! navy 採用診断レポートの可視化資産を、市区町村×業界フィルタ連動で
//! 常設タブ化したもの。レポート (顧客 CSV 軸) と異なり postings DB を
//! 都道府県 / 市区町村 / 業界(job_type) で直接集計する (CSV アップロード不要)。
//!
//! Phase1 スコープ (3 可視化):
//! 1. 給与分布ヒストグラム (ECharts bar + 平均/中央値 markLine)
//! 2. 市区町村別 求人数・給与中央値ランキング表 (業界フィルタ連動)
//! 3. 雇用形態別 給与統計表 (正社員/契約/パート等の中央値/件数)

mod fetch;
mod handlers;
mod render;

// lib.rs から handlers::regional_analysis::* として参照
pub use handlers::{
    regional_emp_salary, regional_job_types, regional_muni_ranking, regional_municipalities,
    regional_salary_histogram, tab_regional_analysis,
};
