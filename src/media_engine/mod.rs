//! キーワード需要ビューア (検索エンジン部) — job_media_engine_rs からの移植 (2026-07-24)。
//!
//! Google Ads Keyword Planner × SerpApi × Gemini で「どこで・何の言葉で・どの媒体に
//! 求人を出すべきか」の判断材料を返す。引き継ぎ資料
//! `検索エンジン部_引き継ぎ_2026-07-24.md` の抽出一覧に準拠し、旧レポート系
//! (/api/case, /api/report) は移植していない。
//!
//! HR_HR 統合での差分:
//! - `serpapi` の月次カウンタ/キャッシュは Turso 一次 ([`store`])
//! - `gemini` はプロセス共通レートリミッタ (12回/分) を共有
//! - `config` の .env フォールバックはカレント直下 (dotenvy と同じ場所)

pub mod config;
pub mod demand;
pub mod gemini;
pub mod geo;
pub mod google_ads;
pub mod handlers;
pub mod keyword_cluster;
pub mod keyword_demand;
pub mod keywords;
pub mod media;
pub mod serp;
pub mod serpapi;
pub mod store;
