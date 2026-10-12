//! Process-wide read caches for the job-copy detail screen.
//!
//! Only content taken from HubSpot is stored (the raw listing record with its history, and the
//! customer name). Authorization is never cached: the handlers check the signed-in user before
//! they look here. Errors and partial answers are never stored.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a listing record (body, images, history, drafts) is reused.
pub(super) const DETAIL_TTL: Duration = Duration::from_secs(60);
/// How long a resolved customer name is reused. It changes rarely and is only a label.
pub(super) const COMPANY_TTL: Duration = Duration::from_secs(6 * 60 * 60);
pub(super) const MAX_ENTRIES: usize = 2_000;

pub(super) struct TtlCache<V> {
    ttl: Duration,
    max: usize,
    map: Mutex<HashMap<String, (Instant, V)>>,
}
impl<V: Clone> TtlCache<V> {
    pub(super) fn new(ttl: Duration, max: usize) -> Self {
        Self {
            ttl,
            max: max.max(1),
            map: Mutex::new(HashMap::new()),
        }
    }
    pub(super) fn get(&self, key: &str) -> Option<V> {
        self.get_at(key, Instant::now())
    }
    pub(super) fn get_at(&self, key: &str, now: Instant) -> Option<V> {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(key) {
            Some((at, value)) if now.saturating_duration_since(*at) < self.ttl => {
                Some(value.clone())
            }
            Some(_) => {
                map.remove(key);
                None
            }
            None => None,
        }
    }
    pub(super) fn put(&self, key: &str, value: V) {
        self.put_at(key, value, Instant::now());
    }
    pub(super) fn put_at(&self, key: &str, value: V, now: Instant) {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if !map.contains_key(key) && map.len() >= self.max {
            let ttl = self.ttl;
            map.retain(|_, (at, _)| now.saturating_duration_since(*at) < ttl);
            if map.len() >= self.max {
                // Still full of live entries: drop the oldest one.
                if let Some(oldest) = map
                    .iter()
                    .min_by_key(|(_, (at, _))| *at)
                    .map(|(key, _)| key.clone())
                {
                    map.remove(&oldest);
                }
            }
        }
        map.insert(key.to_owned(), (now, value));
    }
    pub(super) fn remove(&self, key: &str) {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

/// Listing record answers (60 s) and customer names (6 h), both keyed by the listing ID.
pub(super) struct DetailCaches {
    pub(super) detail: TtlCache<Arc<Value>>,
    pub(super) company: TtlCache<Option<String>>,
    /// Bumped by every write. A read that began before a write must not be stored after it.
    epoch: AtomicU64,
}
impl Default for DetailCaches {
    fn default() -> Self {
        Self::new(DETAIL_TTL, COMPANY_TTL, MAX_ENTRIES)
    }
}
impl DetailCaches {
    pub(super) fn new(detail_ttl: Duration, company_ttl: Duration, max: usize) -> Self {
        Self {
            detail: TtlCache::new(detail_ttl, max),
            company: TtlCache::new(company_ttl, max),
            epoch: AtomicU64::new(0),
        }
    }
    /// Drop what we know about a listing after HubSpot was (or may have been) written for it.
    /// The customer name does not depend on the draft fields, so it stays.
    pub(super) fn forget_listing(&self, id: &str) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.detail.remove(id);
    }
    pub(super) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }
    /// Store a listing read unless a write happened since `epoch` was taken (before the read).
    pub(super) fn store_detail(&self, id: &str, value: Arc<Value>, epoch: u64) {
        if self.epoch() == epoch {
            self.detail.put(id, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hit_expiry_and_removal() {
        let cache: TtlCache<u32> = TtlCache::new(Duration::from_secs(60), 10);
        let t0 = Instant::now();
        cache.put_at("1", 7, t0);
        assert_eq!(cache.get_at("1", t0 + Duration::from_secs(59)), Some(7));
        assert_eq!(cache.get_at("1", t0 + Duration::from_secs(60)), None);
        assert_eq!(cache.len(), 0, "expired entry is dropped on read");
        cache.put_at("2", 8, t0);
        cache.remove("2");
        assert_eq!(cache.get_at("2", t0), None);
    }
    #[test]
    fn capacity_drops_expired_first_then_oldest() {
        let cache: TtlCache<u32> = TtlCache::new(Duration::from_secs(60), 3);
        let t0 = Instant::now();
        cache.put_at("a", 1, t0);
        cache.put_at("b", 2, t0 + Duration::from_secs(10));
        cache.put_at("c", 3, t0 + Duration::from_secs(20));
        // Full of live entries: the oldest ("a") goes.
        cache.put_at("d", 4, t0 + Duration::from_secs(30));
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get_at("a", t0 + Duration::from_secs(30)), None);
        assert_eq!(cache.get_at("b", t0 + Duration::from_secs(30)), Some(2));
        // "b" and "c" are expired by now, so adding "e" clears them and keeps "d".
        cache.put_at("e", 5, t0 + Duration::from_secs(85));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get_at("d", t0 + Duration::from_secs(85)), Some(4));
        // Overwriting an existing key never evicts another one.
        cache.put_at("d", 9, t0 + Duration::from_secs(86));
        assert_eq!(cache.get_at("e", t0 + Duration::from_secs(86)), Some(5));
        assert_eq!(cache.get_at("d", t0 + Duration::from_secs(86)), Some(9));
    }
    #[test]
    fn forgetting_a_listing_keeps_the_customer_name() {
        let caches = DetailCaches::default();
        caches
            .detail
            .put("5", Arc::new(serde_json::json!({"id":"5"})));
        caches.company.put("5", Some("合成運輸株式会社".into()));
        caches.forget_listing("5");
        assert!(caches.detail.get("5").is_none());
        assert_eq!(
            caches.company.get("5"),
            Some(Some("合成運輸株式会社".into()))
        );
    }
    #[test]
    fn a_read_started_before_a_write_is_not_stored_after_it() {
        let caches = DetailCaches::default();
        let before = caches.epoch();
        caches.forget_listing("5");
        caches.store_detail("5", Arc::new(serde_json::json!({"old":true})), before);
        assert!(caches.detail.get("5").is_none());
        caches.store_detail(
            "5",
            Arc::new(serde_json::json!({"new":true})),
            caches.epoch(),
        );
        assert_eq!(caches.detail.get("5").unwrap()["new"], true);
    }
}
