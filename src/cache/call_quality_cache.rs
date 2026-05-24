//! 架電クオリティ用 in-memory TTL キャッシュ
//!
//! 既存 HR_HR が採用している `dashmap` を流用してスレッドセーフに
//! Read-heavy ワークロードを捌く。
//!
//! 設計方針:
//!   - キーは String ベース (ダッシュボード/シート名等)
//!   - 値は JSON 文字列ではなく `serde_json::Value` を保持
//!     (Sheets API レスポンスをそのまま再利用可能)
//!   - TTL 失効は get 時に lazy 削除 (バックグラウンドスレッド不要)
//!   - clear() は管理エンドポイント (?refresh=1) 用

use dashmap::DashMap;
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Entry {
    value: Value,
    expires_at: Instant,
}

pub struct CallQualityCache {
    store: DashMap<String, Entry>,
    default_ttl: Duration,
}

impl CallQualityCache {
    pub fn new(ttl_sec: u64) -> Self {
        Self {
            store: DashMap::new(),
            default_ttl: Duration::from_secs(ttl_sec),
        }
    }

    /// 取得。期限切れなら None を返し、lazy に削除する
    pub fn get(&self, key: &str) -> Option<Value> {
        if let Some(entry) = self.store.get(key) {
            if entry.expires_at > Instant::now() {
                return Some(entry.value.clone());
            }
        }
        // expired or missing -> 削除を試みる
        self.store.remove(key);
        None
    }

    pub fn set(&self, key: impl Into<String>, value: Value) {
        self.set_with_ttl(key, value, self.default_ttl);
    }

    pub fn set_with_ttl(&self, key: impl Into<String>, value: Value, ttl: Duration) {
        self.store.insert(
            key.into(),
            Entry {
                value,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    /// 全削除 (?refresh=1 / 手動リフレッシュ用)
    pub fn clear(&self) -> usize {
        let n = self.store.len();
        self.store.clear();
        n
    }

    /// ヘルスチェック用統計
    pub fn stats(&self) -> CacheStats {
        let now = Instant::now();
        let mut alive = 0_usize;
        for entry in self.store.iter() {
            if entry.expires_at > now {
                alive += 1;
            }
        }
        CacheStats {
            entries_total: self.store.len(),
            entries_alive: alive,
            default_ttl_sec: self.default_ttl.as_secs(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub entries_total: usize,
    pub entries_alive: usize,
    pub default_ttl_sec: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_get_basic() {
        let cache = CallQualityCache::new(60);
        cache.set("a", json!({"n": 1}));
        assert_eq!(cache.get("a"), Some(json!({"n": 1})));
    }

    #[test]
    fn ttl_expiry() {
        let cache = CallQualityCache::new(0);
        cache.set("a", json!(1));
        std::thread::sleep(Duration::from_millis(10));
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn clear_returns_count() {
        let cache = CallQualityCache::new(60);
        cache.set("a", json!(1));
        cache.set("b", json!(2));
        assert_eq!(cache.clear(), 2);
        assert!(cache.get("a").is_none());
    }
}
