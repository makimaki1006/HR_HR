//! 47 都道府県横断比較ビュー (P1-04)
//!
//! ペルソナ C (採用市場リサーチャー) 決定打: 「47 県を一覧する画面が存在しない」
//! を解消する。1 画面で 47 都道府県の主要 KPI を並べて比較できる。
//!
//! # ルート
//! - `GET /tab/comparison` → HTMX partial HTML（テーブル + ECharts 横棒グラフ）
//!
//! # データソース
//! - ローカル `hellowork.db` の `postings` テーブル集計（COUNT/AVG/SUM）
//!
//! # 提供指標 (metric)
//! - `posting_count` : 求人件数
//! - `salary_min_avg` : 月給下限の平均（円）
//! - `seishain_ratio` : 正社員求人比率（%）
//! - `facility_count` : 事業所数（DISTINCT facility_name）
//! - `salary_disclosure_rate` : 給与開示率（%、salary_min > 0 の比率）
//!
//! # 設計原則
//! - HW 掲載求人のみ（民間サイト除外）の旨を明記（feedback_hw_data_scope.md）
//! - 47 都道府県を必ず全件返す（postings に 1 件もない県は 0 件として表示）
//! - 「相関≠因果」: 単純集計のみ。因果推論はしない（feedback_correlation_not_causation.md）

pub mod fetch;
pub mod render;

#[cfg(test)]
mod contract_tests;

pub use render::tab_comparison;
