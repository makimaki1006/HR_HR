//! レコード単位の直列化 (書き込みの TOCTOU 対策)。
//!
//! 書き込みは「現在値を読む → `base` と比べる → HubSpot に PATCH」の 3 段で、複数の要求が同じレコードに
//! 同時に来ると、全員が書く前の値と比べて通ってしまい、最後の書き込みだけが残る (更新の取りこぼし)。
//! これを防ぐため、同じ (オブジェクト種別, id) の書き込みは**プロセス内で 1 つずつ**にする。
//!
//! - 鍵は `deals:123` のような文字列。案件の鍵を先に取り、担当者・会社の鍵は整列した順に取る
//!   (どの要求も同じ順なので、取り合って詰まらない)。
//! - 待つのは最大 `wait`。待ちきれなければ呼び出し側が 503 `record_busy` を返す (黙って並ばせない)。
//! - 使われなくなった鍵は表から消す (表は際限なく増えない)。
//! - **単一インスタンス前提**: 鍵はプロセス内のメモリにある。Render が 2 台以上に増える場合は分散ロックが要る
//!   (`docs/architecture/headless-crm-design.md` §10〜§11 の実装メモ)。

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// 1 回の取得で待つ最長 (既定)
pub const DEFAULT_LOCK_WAIT: Duration = Duration::from_secs(10);

type Slot = Arc<AsyncMutex<()>>;

#[derive(Default)]
pub struct RecordLocks {
    map: Mutex<HashMap<String, Slot>>,
}

impl std::fmt::Debug for RecordLocks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordLocks")
            .field("entries", &self.len())
            .finish()
    }
}

/// 取った鍵の束。落とすと解放し、誰も使っていない鍵は表から消える
pub struct HeldLocks {
    locks: Arc<RecordLocks>,
    held: Vec<(String, Slot, Option<OwnedMutexGuard<()>>)>,
}

/// 鍵が取れなかった (待ちきれなかった)
#[derive(Debug, PartialEq, Eq)]
pub struct RecordBusy;

/// 本番用の共有の表 (要求側と再送 worker が同じものを使う)
pub fn shared() -> Arc<RecordLocks> {
    static SHARED: OnceLock<Arc<RecordLocks>> = OnceLock::new();
    SHARED
        .get_or_init(|| Arc::new(RecordLocks::default()))
        .clone()
}

pub fn key(object: &str, id: &str) -> String {
    format!("{object}:{id}")
}

impl RecordLocks {
    /// 表にある鍵の数 (テスト・診断用)
    pub fn len(&self) -> usize {
        self.map.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn entry(&self, k: &str) -> Slot {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        m.entry(k.to_string()).or_default().clone()
    }

    /// 誰も持たず誰も待っていなければ表から消す
    /// (`slot` は呼び出し側の 1 本。表の 1 本と合わせて 2 本なら他に誰もいない)
    fn cleanup(&self, k: &str, slot: &Slot) {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if m.get(k).is_some_and(|c| Arc::ptr_eq(c, slot)) && Arc::strong_count(slot) <= 2 {
            m.remove(k);
        }
    }

    /// `keys` を整列した順に取る。全体で最大 `wait` だけ待つ
    pub async fn acquire(
        self: &Arc<Self>,
        keys: impl IntoIterator<Item = String>,
        wait: Duration,
    ) -> Result<HeldLocks, RecordBusy> {
        let mut held = HeldLocks {
            locks: self.clone(),
            held: Vec::new(),
        };
        held.extend(keys, wait).await?;
        Ok(held)
    }
}

impl HeldLocks {
    /// すでに持っている鍵に加えて `keys` を (整列した順に) 取る。すでに持っている鍵は飛ばす
    pub async fn extend(
        &mut self,
        keys: impl IntoIterator<Item = String>,
        wait: Duration,
    ) -> Result<(), RecordBusy> {
        let sorted: BTreeSet<String> = keys.into_iter().collect();
        let deadline = tokio::time::Instant::now() + wait;
        for k in sorted {
            if self.held.iter().any(|(h, _, _)| *h == k) {
                continue;
            }
            let slot = self.locks.entry(&k);
            match tokio::time::timeout_at(deadline, slot.clone().lock_owned()).await {
                Ok(g) => self.held.push((k, slot, Some(g))),
                Err(_) => {
                    self.locks.cleanup(&k, &slot);
                    return Err(RecordBusy);
                }
            }
        }
        Ok(())
    }
}

impl Drop for HeldLocks {
    fn drop(&mut self) {
        // 取った逆順に放す
        while let Some((k, slot, g)) = self.held.pop() {
            drop(g);
            self.locks.cleanup(&k, &slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Duration = Duration::from_millis(200);

    #[tokio::test]
    async fn 同じ鍵は待たされ_待ちきれなければ_busy_で_表に残らない() {
        let l = Arc::new(RecordLocks::default());
        let a = l.acquire([key("deals", "1")], W).await.unwrap();
        let r = l
            .acquire([key("deals", "1")], Duration::from_millis(30))
            .await;
        assert_eq!(r.err(), Some(RecordBusy));
        assert_eq!(l.len(), 1);
        drop(a);
        assert!(l.is_empty(), "解放後は表が空になる");
    }

    #[tokio::test]
    async fn 逆順で要求しても整列されるので詰まらない() {
        let l = Arc::new(RecordLocks::default());
        let (l1, l2) = (l.clone(), l.clone());
        let t1 = tokio::spawn(async move {
            for _ in 0..200 {
                let _g = l1
                    .acquire(
                        [key("contacts", "9"), key("companies", "8")],
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap();
                tokio::task::yield_now().await;
            }
        });
        let t2 = tokio::spawn(async move {
            for _ in 0..200 {
                let _g = l2
                    .acquire(
                        [key("companies", "8"), key("contacts", "9")],
                        Duration::from_secs(5),
                    )
                    .await
                    .unwrap();
                tokio::task::yield_now().await;
            }
        });
        t1.await.unwrap();
        t2.await.unwrap();
        assert!(l.is_empty());
    }

    #[tokio::test]
    async fn 異なる鍵は互いに待たない() {
        let l = Arc::new(RecordLocks::default());
        let _a = l.acquire([key("deals", "1")], W).await.unwrap();
        let _b = l.acquire([key("deals", "2")], W).await.unwrap();
        assert_eq!(l.len(), 2);
    }
}
