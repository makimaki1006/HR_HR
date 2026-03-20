//! 時系列トレンド分析タブ
//! HW過去データ（20月次スナップショット）のTurso集計テーブルを可視化
//! 4サブタブ: 量の変化 / 質の変化 / 構造の変化 / シグナル

mod fetch;
mod handlers;
mod helpers;
mod render;

pub use handlers::{tab_trend, trend_subtab};
