//! 架電キュー (`GET /api/crm/call-queue`) の短いキャッシュ。
//!
//! - **1 ページの中身** ([`QueuePageKey`] → `QueueCore`): 有効期間 [`QUEUE_PAGE_TTL`] (30 秒)、最大 [`QUEUE_PAGE_MAX`] 件。
//!   キーは「パイプライン・絞り込み・並び・**解決済みの担当者** (`me` は本人の owner ID に直したもの)・今日 (JST)・
//!   ページの位置 (段階と after)」。担当者がキーに入るので、A さん (担当 111) のページを B さん (担当 222) に返すことはない。
//!   中身は利用者によらない部分だけで、cursor (本人のメールに束縛した署名) と `scope` (役割・所属チーム) は
//!   要求ごとに作り直す。
//! - **段階の件数** ([`QueueCountKey`] → 件数): 先頭ページで総数を出すための件数 Search の結果。有効期間 [`QUEUE_COUNT_TTL`]
//!   (60 秒)、最大 [`QUEUE_COUNT_MAX`] 件。
//! - **認可は毎回の要求で、キャッシュを見る前に行う** (`rbac::authorize`。架電キューは全員が全件を見られる決まりなので、
//!   認可を通った人には担当者の絞り込み以外の違いは無い)。
//! - 入れるのは、関連の読み取りに欠け (`partial.failed`) が無く、数えるべき総数を数えられた応答だけ。
//! - `?fresh=1` はキャッシュを見ずに HubSpot から読み直し、結果で置き換える。
//! - 満杯なら、期限切れを捨ててから最も古く入れたものを捨てる。

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 1 ページの有効期間
pub const QUEUE_PAGE_TTL: Duration = Duration::from_secs(30);
/// 1 ページのキャッシュの最大件数
pub const QUEUE_PAGE_MAX: usize = 500;
/// 段階の件数の有効期間
pub const QUEUE_COUNT_TTL: Duration = Duration::from_secs(60);
/// 段階の件数のキャッシュの最大件数
pub const QUEUE_COUNT_MAX: usize = 1_000;

/// 1 ページのキー。`owner` は解決済み (`all` / `unassigned` / `id:<owner id>`)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueuePageKey {
    pub filter: QueueFilterKey,
    pub limit: u32,
    /// (段階, after)。先頭ページは (0, None)
    pub start: (u32, Option<u64>),
}

/// 絞り込みのキー (件数のキャッシュと共通。並びは段階の並びを決めるので入れる)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueueFilterKey {
    pub pipeline: String,
    pub q: Option<String>,
    /// 昇順・重複なし (空 = パイプラインの全部)
    pub stages: Vec<String>,
    /// 解決済みの担当者 (`all` / `unassigned` / `id:<owner id>`)
    pub owner: String,
    pub due: String,
    pub sort: String,
    /// 今日 (JST) の UTC 0 時の ms (「今日以前」の条件が変わるため)
    pub today_ms: i64,
    pub next_from: Option<String>,
    pub next_to: Option<String>,
    pub last_from: Option<String>,
    pub last_to: Option<String>,
}

/// 段階の件数のキー
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueueCountKey {
    pub filter: QueueFilterKey,
    pub phase: u32,
}

/// 期限付き・件数上限付きのキャッシュ
pub struct TtlCache<K, V> {
    inner: Mutex<HashMap<K, (Instant, u64, V)>>,
    seq: std::sync::atomic::AtomicU64,
    ttl: Duration,
    max: usize,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    pub fn new(ttl: Duration, max: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            seq: std::sync::atomic::AtomicU64::new(0),
            ttl,
            max: max.max(1),
        }
    }

    /// 有効期間内なら返す (期限切れは捨てる)
    pub fn get(&self, key: &K) -> Option<V> {
        let mut g = self.inner.lock().ok()?;
        match g.get(key) {
            Some((at, _, v)) if at.elapsed() < self.ttl => Some(v.clone()),
            Some(_) => {
                g.remove(key);
                None
            }
            None => None,
        }
    }

    pub fn insert(&self, key: K, value: V) {
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        let ttl = self.ttl;
        g.retain(|_, (at, _, _)| at.elapsed() < ttl);
        while g.len() >= self.max && !g.contains_key(&key) {
            let Some(oldest) = g
                .iter()
                .min_by_key(|(_, (_, seq, _))| *seq)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            g.remove(&oldest);
        }
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        g.insert(key, (Instant::now(), seq, value));
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 期限切れは返さず_満杯なら最も古いものを捨てる() {
        let c: TtlCache<u32, &str> = TtlCache::new(Duration::from_millis(50), 2);
        c.insert(1, "a");
        c.insert(2, "b");
        c.insert(3, "c");
        assert_eq!(c.len(), 2);
        assert_eq!(c.get(&1), None, "最も古いものが捨てられた");
        assert_eq!(c.get(&2), Some("b"));
        assert_eq!(c.get(&3), Some("c"));
        // 同じキーの入れ直しは件数を増やさない
        c.insert(3, "c2");
        assert_eq!(c.get(&3), Some("c2"));
        assert_eq!(c.get(&2), Some("b"));
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(c.get(&2), None);
        assert!(c.is_empty() || c.len() == 1);
    }
}
