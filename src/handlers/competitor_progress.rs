//! 競合調査の進捗 (`GET /api/competitor/progress/{id}`)。
//!
//! 画面が送信前に作る乱数の ID (`progress_id`) をレポート作成の要求と一緒に送り、サーバは段階が進むたびに
//! この記録を更新する。画面は同じ ID で短い間隔で取り直す。記録はこのプロセスのメモリだけ (単一インスタンス前提)、
//! 作った本人のメールでしか読めず、10 分で捨てる。HubSpot・Turso・環境変数は使わない。
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

/// 記録の保持時間。レポートの最悪待ち (約 2 分) より十分長い。
const TTL: Duration = Duration::from_secs(10 * 60);
/// 記録数の上限 (メモリの上限)。超えたら古い順に捨てる。
const CAP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Csv,
    Indeed,
    GoogleSuggest,
    GoogleDemand,
    Population,
    Build,
}

impl Stage {
    pub(super) fn label(self) -> &'static str {
        match self {
            Stage::Csv => "CSVを読み込み中",
            Stage::Indeed => "Indeed採用市場データを確認中",
            Stage::GoogleSuggest => "Google: 関連する検索語を取得中",
            Stage::GoogleDemand => "Google: 検索数(最大48か月)を取得中",
            Stage::Population => "人口・地域データを取得中",
            Stage::Build => "レポートを作成中",
        }
    }
}

/// 実行する段階の並び。Google を使わないときは Google の 2 段を数えない。
pub(super) fn plan(google: bool) -> Vec<Stage> {
    let mut stages = vec![Stage::Csv, Stage::Indeed];
    if google {
        stages.push(Stage::GoogleSuggest);
        stages.push(Stage::GoogleDemand);
    }
    stages.push(Stage::Population);
    stages.push(Stage::Build);
    stages
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CompetitorProgress {
    /// 段階の総数。
    pub total: u32,
    /// 終わった段階の数 (0..=total)。
    pub completed: u32,
    /// いま実行している段階の名前。終わったときは「完了」。
    pub label: String,
    pub finished: bool,
}

struct Entry {
    owner: String,
    at: Instant,
    stages: Vec<Stage>,
    completed: usize,
    finished: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn normalize_user(user: &str) -> String {
    user.trim().to_lowercase()
}

pub struct ProgressStore {
    ttl: Duration,
    cap: usize,
    inner: Mutex<HashMap<String, Entry>>,
}

impl ProgressStore {
    pub fn new(ttl: Duration, cap: usize) -> Self {
        Self {
            ttl,
            cap,
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn purge(&self, map: &mut HashMap<String, Entry>, now: Instant) {
        let ttl = self.ttl;
        map.retain(|_, e| now.saturating_duration_since(e.at) <= ttl);
    }

    /// 記録を作る。同じ ID が他人のものなら作らず false (他人の記録を上書きしない)。
    pub(super) fn begin(&self, owner: &str, id: &str, stages: Vec<Stage>, now: Instant) -> bool {
        let owner = normalize_user(owner);
        let mut map = lock(&self.inner);
        self.purge(&mut map, now);
        if map.get(id).is_some_and(|e| e.owner != owner) {
            return false;
        }
        while map.len() >= self.cap && !map.contains_key(id) {
            let Some(oldest) = map.iter().min_by_key(|(_, e)| e.at).map(|(k, _)| k.clone()) else {
                break;
            };
            map.remove(&oldest);
        }
        map.insert(
            id.to_owned(),
            Entry {
                owner,
                at: now,
                stages,
                completed: 0,
                finished: false,
            },
        );
        true
    }

    /// 指定の段階を実行中にする (それより前の段階は終わった扱い)。計画にない段階は無視する。
    pub(super) fn enter(&self, owner: &str, id: &str, stage: Stage, now: Instant) {
        let owner = normalize_user(owner);
        let mut map = lock(&self.inner);
        if let Some(e) = map.get_mut(id).filter(|e| e.owner == owner) {
            if let Some(i) = e.stages.iter().position(|s| *s == stage) {
                e.completed = e.completed.max(i);
                e.at = now;
            }
        }
    }

    /// すべての段階が終わった。
    pub(super) fn finish(&self, owner: &str, id: &str, now: Instant) {
        let owner = normalize_user(owner);
        let mut map = lock(&self.inner);
        if let Some(e) = map.get_mut(id).filter(|e| e.owner == owner) {
            e.completed = e.stages.len();
            e.finished = true;
            e.at = now;
        }
    }

    /// 本人が読む。他人・期限切れ・存在しない ID はどれも `None` (区別しない)。
    pub fn get(&self, owner: &str, id: &str, now: Instant) -> Option<CompetitorProgress> {
        let mut map = lock(&self.inner);
        self.purge(&mut map, now);
        let e = map.get(id).filter(|e| e.owner == normalize_user(owner))?;
        let label = if e.finished {
            "完了"
        } else {
            e.stages.get(e.completed).map_or("完了", |s| s.label())
        };
        Some(CompetitorProgress {
            total: e.stages.len() as u32,
            completed: e.completed as u32,
            label: label.to_owned(),
            finished: e.finished,
        })
    }
}

pub(super) fn store() -> &'static ProgressStore {
    static STORE: OnceLock<ProgressStore> = OnceLock::new();
    STORE.get_or_init(|| ProgressStore::new(TTL, CAP))
}

/// 進捗を記録する側の持ち物。ID が無い (旧画面) ときは何もしない。
#[derive(Clone)]
pub(super) struct Tracker {
    target: Option<(String, String)>,
}

impl Tracker {
    pub(super) fn none() -> Self {
        Self { target: None }
    }

    pub(super) fn start(owner: &str, id: Option<&str>, google: bool) -> Self {
        let Some(id) = id else {
            return Self::none();
        };
        if store().begin(owner, id, plan(google), Instant::now()) {
            Self {
                target: Some((owner.to_owned(), id.to_owned())),
            }
        } else {
            Self::none()
        }
    }

    pub(super) fn enter(&self, stage: Stage) {
        if let Some((owner, id)) = &self.target {
            store().enter(owner, id, stage, Instant::now());
        }
    }

    pub(super) fn finish(&self) {
        if let Some((owner, id)) = &self.target {
            store().finish(owner, id, Instant::now());
        }
    }
}

/// 画面が作る ID の形 (16 進 16〜64 桁)。それ以外は記録しない。
pub(super) fn valid_id(raw: &str) -> bool {
    (16..=64).contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    fn s() -> ProgressStore {
        ProgressStore::new(Duration::from_secs(600), 3)
    }

    #[test]
    fn plan_has_six_stages_with_google_and_four_without() {
        assert_eq!(plan(true).len(), 6);
        let without = plan(false);
        assert_eq!(without.len(), 4);
        assert!(!without.contains(&Stage::GoogleSuggest));
        assert!(!without.contains(&Stage::GoogleDemand));
    }

    #[test]
    fn stages_advance_and_labels_follow() {
        let st = s();
        let now = Instant::now();
        assert!(st.begin("a@x", ID, plan(true), now));
        let p = st.get("a@x", ID, now).unwrap();
        assert_eq!(
            (p.total, p.completed, p.label.as_str()),
            (6, 0, "CSVを読み込み中")
        );
        st.enter("a@x", ID, Stage::GoogleSuggest, now);
        let p = st.get("a@x", ID, now).unwrap();
        assert_eq!(
            (p.completed, p.label.as_str()),
            (2, "Google: 関連する検索語を取得中")
        );
        st.enter("a@x", ID, Stage::GoogleDemand, now);
        assert_eq!(st.get("a@x", ID, now).unwrap().completed, 3);
        st.enter("a@x", ID, Stage::Indeed, now); // 戻らない
        assert_eq!(st.get("a@x", ID, now).unwrap().completed, 3);
        st.finish("a@x", ID, now);
        let p = st.get("a@x", ID, now).unwrap();
        assert_eq!(
            (p.completed, p.total, p.finished, p.label.as_str()),
            (6, 6, true, "完了")
        );
    }

    #[test]
    fn without_google_the_stage_is_skipped_and_total_is_four() {
        let st = s();
        let now = Instant::now();
        st.begin("a@x", ID, plan(false), now);
        st.enter("a@x", ID, Stage::GoogleSuggest, now); // 計画にない段階は無視
        assert_eq!(st.get("a@x", ID, now).unwrap().completed, 0);
        st.enter("a@x", ID, Stage::Population, now);
        let p = st.get("a@x", ID, now).unwrap();
        assert_eq!((p.total, p.completed), (4, 2));
        assert_eq!(p.label, "人口・地域データを取得中");
    }

    #[test]
    fn other_users_cannot_read_or_overwrite() {
        let st = s();
        let now = Instant::now();
        assert!(st.begin("a@x", ID, plan(true), now));
        assert!(st.get("b@x", ID, now).is_none());
        assert!(!st.begin("b@x", ID, plan(false), now));
        st.enter("b@x", ID, Stage::Build, now);
        assert_eq!(st.get("A@x ", ID, now).unwrap().completed, 0);
    }

    #[test]
    fn entries_expire_and_cap_drops_oldest() {
        let st = s();
        let t0 = Instant::now();
        st.begin("a@x", ID, plan(true), t0);
        assert!(st.get("a@x", ID, t0 + Duration::from_secs(601)).is_none());
        for i in 0..4 {
            st.begin(
                "a@x",
                &format!("{i:032x}"),
                plan(true),
                t0 + Duration::from_secs(i),
            );
        }
        assert!(st
            .get("a@x", &format!("{:032x}", 0), t0 + Duration::from_secs(5))
            .is_none());
        assert!(st
            .get("a@x", &format!("{:032x}", 3), t0 + Duration::from_secs(5))
            .is_some());
    }

    #[test]
    fn id_shape() {
        assert!(valid_id(ID));
        assert!(!valid_id("short"));
        assert!(!valid_id(&"g".repeat(32)));
        assert!(!valid_id(&"a".repeat(65)));
    }
}
