//! Indeed 採用市場データの 2 つの出口。
//!
//! * `/tab/indeed`    … 社内。分解まで出す。「なぜ」に答えられる形にする
//! * `/report/indeed` … 顧客。定点と図と散文だけ。商談の入口として使う
//!
//! 集計は必ず [`crate::indeed::aggregate`] を通す。ここで計算し直さない。
//!
//! # 顧客側は既定で閉じている
//! 社外に配ってよいかの確認が済んでいないため、`/report/indeed` は
//! `INDEED_PUBLIC=on` が立つまで 404 を返す（[`crate::indeed::public_report_enabled`]）。

pub mod render;
pub mod report;
pub mod tab;
pub mod title;

use std::sync::Arc;

use axum::{routing::get, Router};

use crate::AppState;

/// Indeed のルーター。
///
/// `build_app()` の `protected_routes` チェーンに、認証の `route_layer` より
/// **前** に merge すること。後ろに置くと認証が掛からない。
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tab/indeed", get(tab::tab_indeed))
        // 職種 1 つを深く見る。名前で参照する（この分析層の主キーは職種名）
        .route("/tab/indeed/title", get(title::tab_indeed_title))
        .route("/report/indeed", get(report::report_indeed))
}
