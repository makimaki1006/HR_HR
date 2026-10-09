//! 書き込み操作の状態のプロセス内表 (`GET /api/crm/operations/{id}` をメモリから返すため)。
//!
//! 202 のあと、画面は操作が終わるまで状態を何度も照会する。台帳 (監査 Turso) を毎回 SELECT すると、
//! 台帳が詰まる局面 (= 202 が増える局面) ほど Turso の読み取りが増える (2026-10-09 の負荷試験で、
//! 202 が 1,741 件 → 照会 78,593 回 → SELECT 約 19.5 万)。そこで、台帳を更新した場所
//! (受付・再送 worker・管理者の再試行/破棄) が、同じ瞬間にこの表も更新する。
//!
//! - 正本は台帳。この表は**台帳の写し**で、持つのは状態・回数・直近のエラーコード・次の再送時刻・持ち主だけ
//!   (操作の中身 = payload や値は持たない)。
//! - 上限件数 ([`MAX_ENTRIES`]) と有効期間 ([`TTL`]) がある。外れたもの・再起動で消えたものは、
//!   呼び出し側が台帳を 1 回読んで [`OpStatusCache::put`] で入れ直す。
//! - **単一インスタンス前提** (record_lock と同じ)。複数台になると他の台の更新が見えないので、
//!   台数を増やすときは有効期間を短くするか表をやめる。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 表に持つ最大件数
pub const MAX_ENTRIES: usize = 20_000;
/// 最後に更新されてから有効な時間。これを過ぎたら台帳を読み直す
pub const TTL: Duration = Duration::from_secs(60 * 60);

/// 照会の応答に必要な分だけ
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpState {
    pub operator_email: String,
    /// `pending` / `retrying` / `saved` / `failed` (API の状態名。台帳の `status` ではない)
    pub status: String,
    pub attempts: i64,
    pub last_error_code: String,
    pub next_retry_at: String,
}

pub struct OpStatusCache {
    map: Mutex<HashMap<String, (Instant, OpState)>>,
    ttl: Duration,
    max: usize,
}

impl Default for OpStatusCache {
    fn default() -> Self {
        Self::new(TTL, MAX_ENTRIES)
    }
}

impl std::fmt::Debug for OpStatusCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpStatusCache")
            .field("entries", &self.len())
            .finish()
    }
}

/// 本番用の共有の表 (要求側と再送 worker が同じものを使う)
pub fn shared() -> Arc<OpStatusCache> {
    static SHARED: OnceLock<Arc<OpStatusCache>> = OnceLock::new();
    SHARED
        .get_or_init(|| Arc::new(OpStatusCache::default()))
        .clone()
}

impl OpStatusCache {
    pub fn new(ttl: Duration, max: usize) -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            ttl,
            max: max.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.map.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 有効期間内のものだけ返す
    pub fn get(&self, operation_id: &str) -> Option<OpState> {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        match m.get(operation_id) {
            Some((at, s)) if at.elapsed() < self.ttl => Some(s.clone()),
            Some(_) => {
                m.remove(operation_id);
                None
            }
            None => None,
        }
    }

    /// 入れる (同じ id は置き換え、有効期間を数え直す)。満杯なら期限切れ、それでも満杯なら古い 1 割を捨てる
    pub fn put(&self, operation_id: &str, state: OpState) {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if m.len() >= self.max && !m.contains_key(operation_id) {
            let ttl = self.ttl;
            m.retain(|_, (at, _)| at.elapsed() < ttl);
            if m.len() >= self.max {
                let mut ages: Vec<(Instant, String)> =
                    m.iter().map(|(k, (at, _))| (*at, k.clone())).collect();
                ages.sort();
                for (_, k) in ages.into_iter().take((self.max / 10).max(1)) {
                    m.remove(&k);
                }
            }
        }
        m.insert(operation_id.to_string(), (Instant::now(), state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(status: &str) -> OpState {
        OpState {
            operator_email: "u@example.com".into(),
            status: status.into(),
            attempts: 1,
            last_error_code: String::new(),
            next_retry_at: String::new(),
        }
    }

    #[test]
    fn 入れたものは返り_置き換えられ_期限が過ぎたら消える() {
        let c = OpStatusCache::new(Duration::from_millis(40), 10);
        assert!(c.get("a").is_none());
        c.put("a", st("pending"));
        assert_eq!(c.get("a").unwrap().status, "pending");
        c.put("a", st("saved"));
        assert_eq!(c.get("a").unwrap().status, "saved");
        std::thread::sleep(Duration::from_millis(60));
        assert!(c.get("a").is_none(), "期限切れは台帳に戻る");
        assert!(c.is_empty(), "期限切れは表からも消える");
    }

    #[test]
    fn 件数の上限を超えない_古いものから捨てる() {
        let c = OpStatusCache::new(Duration::from_secs(60), 20);
        for i in 0..20 {
            c.put(&format!("op-{i}"), st("pending"));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(c.len(), 20);
        c.put("op-new", st("pending"));
        assert!(c.len() <= 20, "上限を超えない: {}", c.len());
        assert!(c.get("op-new").is_some());
        assert!(c.get("op-0").is_none(), "いちばん古いものを捨てる");
        assert!(c.get("op-19").is_some());
    }
}
