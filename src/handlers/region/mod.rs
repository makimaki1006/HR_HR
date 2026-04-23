//! 地域カルテ (Region Karte) ハンドラ
//!
//! RESAS的統合ビュー。市区町村選択 → 全指標集約表示。
//! 既存6タブ (market/jobmap/analysis/competitive/diagnostic/company/survey) とは独立して、
//! SSDSE-A Phase A (構造指標) + Agoop Phase B (人流) + HW求人集計 + 示唆エンジン出力
//! を1画面に並べる。
//!
//! 設計原則:
//! - 既存タブのHTML/JS/CSS は一切変更しない（readonly）
//! - 既存 `switchLocation` 関数をそのまま再利用（HTMLトップの pref/muni セレクト共有）
//! - ECharts は `data-chart-config` 属性パターンで `app.js` が自動初期化
//! - Leaflet 地図は本タブに含めない（既存 /tab/jobmap で提供）
//! - 全データ読み取りのみ。書き込みは一切行わない

pub mod karte;

// Team γ 監査テスト (2026-04-22): api_region_karte 契約 + データフロー逆証明
#[cfg(test)]
mod karte_audit_test;

pub use karte::{api_region_karte, tab_region_karte};
