//! キャッシュの有効期間を測る時計。本番は `Instant::now()`、テストは手で進める時計に差し替えられる。
//!
//! 実時間の `sleep` で期限切れを待つテストは、負荷の高い CI で「待ったつもりが足りない / 待ちすぎる」
//! ことがある (有効期間より短い間隔の前提が崩れる)。期限の判定を時計に通しておけば、
//! テストは時計を進めるだけで決まった順序を再現できる。

use std::time::{Duration, Instant};

#[cfg(test)]
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// 現在時刻の取り方
#[derive(Clone, Default)]
pub struct Clock {
    #[cfg(test)]
    manual: Option<Arc<ManualClock>>,
}

/// テストで手で進める時計
#[cfg(test)]
pub struct ManualClock {
    base: Instant,
    elapsed_ms: AtomicU64,
}

impl Clock {
    pub fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(m) = &self.manual {
            return m.base + Duration::from_millis(m.elapsed_ms.load(Ordering::SeqCst));
        }
        Instant::now()
    }

    /// `since` から今までの経過時間
    pub fn since(&self, since: Instant) -> Duration {
        self.now().saturating_duration_since(since)
    }

    /// 手で進める時計 (テスト用)。返した [`ManualClock`] の `advance` で進める
    #[cfg(test)]
    pub fn manual() -> (Self, Arc<ManualClock>) {
        let m = Arc::new(ManualClock {
            base: Instant::now(),
            elapsed_ms: AtomicU64::new(0),
        });
        (
            Self {
                manual: Some(m.clone()),
            },
            m,
        )
    }
}

#[cfg(test)]
impl ManualClock {
    pub fn advance(&self, d: Duration) {
        self.elapsed_ms
            .fetch_add(d.as_millis() as u64, Ordering::SeqCst);
    }
}
