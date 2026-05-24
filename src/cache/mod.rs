//! 架電クオリティ用 TTL キャッシュモジュール群
//!
//! 既存 db/cache.rs (AppCache) は HelloWork タブ用の汎用キャッシュ。
//! こちらは Google Sheets レスポンスを Value 単位で持つ専用キャッシュで、
//! 1 時間 TTL を前提とした read-heavy ワークロード向け。

pub mod call_quality_cache;
