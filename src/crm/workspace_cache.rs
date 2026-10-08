//! 架電ワークスペースの応答 (`GET /api/crm/workspace/deals/{id}`) の短いキャッシュ。
//!
//! - キー: 案件 ID + 選んだ項目 (案件・担当者・会社。並びは問わない)
//! - 有効期間 [`WORKSPACE_CACHE_TTL`] (60 秒)。件数は [`WORKSPACE_CACHE_MAX`] まで。
//!   入れるときに期限切れを捨て、それでも満杯なら最も長く使われていないものを捨てる (LRU)
//! - 利用者をまたいで共有する。**認可 (rbac とレコード単位の関門) は毎回の要求で、キャッシュを返す前に行う**
//!   (関門のために案件の本体 [`CachedWorkspace::deal`] も一緒に持つ)。その判定は `workspace` 側
//! - 入れるのは、欠けた部分 (`partial`) の無い成功した応答だけ
//! - `?fresh=1` (画面の「最新にする」・通話が終わった後の読み直し) と、今後の書き込みのあとは
//!   [`WorkspaceCache::invalidate_deal`] でその案件の全キーを捨てる

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use super::workspace::WorkspaceResponse;
use crate::hubspot::HubSpotRecord;

/// 有効期間
pub const WORKSPACE_CACHE_TTL: chrono::Duration = chrono::Duration::seconds(60);
/// 最大件数
pub const WORKSPACE_CACHE_MAX: usize = 500;

/// 時計 (テストで差し替える)
pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceCacheKey {
    deal_id: String,
    deal_props: Vec<String>,
    contact_props: Vec<String>,
    company_props: Vec<String>,
}

impl WorkspaceCacheKey {
    /// 選んだ項目は並べ替えて重複を除く (並びが違っても応答は同じ)
    pub fn new(deal_id: &str, deal: &[String], contact: &[String], company: &[String]) -> Self {
        let norm = |v: &[String]| {
            let mut v = v.to_vec();
            v.sort();
            v.dedup();
            v
        };
        Self {
            deal_id: deal_id.to_string(),
            deal_props: norm(deal),
            contact_props: norm(contact),
            company_props: norm(company),
        }
    }
}

/// キャッシュした応答 1 件
#[derive(Debug)]
pub struct CachedWorkspace {
    /// `cached` は false のまま持つ (返すときに true にする)
    pub body: WorkspaceResponse,
    /// レコード単位の関門 (`deal_in_queue`) に使う案件の本体
    pub deal: HubSpotRecord,
    pub stored_at: DateTime<Utc>,
}

struct Slot {
    entry: Arc<CachedWorkspace>,
    last_used: u64,
}

#[derive(Default)]
struct Inner {
    map: HashMap<WorkspaceCacheKey, Slot>,
    tick: u64,
    /// 捨てた回数。読み始めより後に捨てられていたら、その読み取りの結果は入れない
    /// (通話の前から走っていた古い読み取りが、通話後に読み直した内容を上書きしないように)
    epoch: u64,
}

pub struct WorkspaceCache {
    inner: Mutex<Inner>,
    ttl: chrono::Duration,
    max: usize,
    clock: Clock,
}

impl Default for WorkspaceCache {
    fn default() -> Self {
        Self::with_clock(WORKSPACE_CACHE_TTL, WORKSPACE_CACHE_MAX, Arc::new(Utc::now))
    }
}

impl WorkspaceCache {
    pub fn with_clock(ttl: chrono::Duration, max: usize, clock: Clock) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            ttl,
            max: max.max(1),
            clock,
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    fn fresh(&self, e: &CachedWorkspace, now: DateTime<Utc>) -> bool {
        now >= e.stored_at && now - e.stored_at < self.ttl
    }

    /// 有効期間内なら返す (期限切れは捨てる)。認可はここでは見ない (呼び出し側が毎回行う)
    pub fn get(&self, key: &WorkspaceCacheKey) -> Option<Arc<CachedWorkspace>> {
        let now = self.now();
        let mut g = self.inner.lock().ok()?;
        g.tick += 1;
        let tick = g.tick;
        match g.map.get_mut(key) {
            Some(slot) if self.fresh(&slot.entry, now) => {
                slot.last_used = tick;
                Some(slot.entry.clone())
            }
            Some(_) => {
                g.map.remove(key);
                None
            }
            None => None,
        }
    }

    /// 読み始める前に取っておく値 ([`Self::insert`] に渡す)
    pub fn epoch(&self) -> u64 {
        self.inner.lock().map(|g| g.epoch).unwrap_or(u64::MAX)
    }

    /// 入れる。呼び出し側は欠けの無い成功した応答だけを渡す。`read_epoch` は読み始める前の [`Self::epoch`]。
    /// 読んでいる間に [`Self::invalidate_deal`] があったら入れない (入れたら true)
    pub fn insert(
        &self,
        key: WorkspaceCacheKey,
        body: WorkspaceResponse,
        deal: HubSpotRecord,
        read_epoch: u64,
    ) -> bool {
        let now = self.now();
        let Ok(mut g) = self.inner.lock() else {
            return false;
        };
        if g.epoch != read_epoch {
            return false;
        }
        g.map.retain(|_, s| self.fresh(&s.entry, now));
        while g.map.len() >= self.max && !g.map.contains_key(&key) {
            let Some(oldest) = g
                .map
                .iter()
                .min_by_key(|(_, s)| s.last_used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            g.map.remove(&oldest);
        }
        g.tick += 1;
        let tick = g.tick;
        g.map.insert(
            key,
            Slot {
                entry: Arc::new(CachedWorkspace {
                    body,
                    deal,
                    stored_at: now,
                }),
                last_used: tick,
            },
        );
        true
    }

    /// その案件の全キー (選んだ項目の違いを問わず) を捨てる。捨てた件数を返す。
    /// `?fresh=1` の読み直しと、今後の HubSpot への書き込みの後に呼ぶ
    pub fn invalidate_deal(&self, deal_id: &str) -> usize {
        let Ok(mut g) = self.inner.lock() else {
            return 0;
        };
        g.epoch += 1;
        let before = g.map.len();
        g.map.retain(|k, _| k.deal_id != deal_id);
        before - g.map.len()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.map.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
