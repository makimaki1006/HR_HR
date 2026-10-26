//! 案件ごとの応募日と掲載中の求人数を、他の画面（コンサルKPI）が読むための読み取り専用の公開口。
//! 案件管理が集計し終えたとき（と起動時に保存データを戻したとき）に、その時点の結果をそのまま置く。
//! 新しい読み取り・保存先・更新の仕組みは持たない。
use super::todo_signals::Facts;
use chrono::{DateTime, NaiveDate, Utc};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// 1案件の応募日（応募 ID は重複排除済み）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DealApplications {
    pub days: Vec<NaiveDate>,
    /// 応募日が分からない応募の数。0 でないとき「応募が無い」とは言えない。
    pub unknown_dates: usize,
    /// いま掲載中の求人の数（案件管理が数えたもの）。
    pub live: usize,
}

#[derive(Debug)]
pub struct Published {
    pub built_at: DateTime<Utc>,
    pub deals: BTreeMap<String, DealApplications>,
}

static CURRENT: RwLock<Option<Arc<Published>>> = RwLock::new(None);

pub(super) fn publish(built_at: DateTime<Utc>, facts: &BTreeMap<String, Facts>) {
    let deals = facts
        .iter()
        .map(|(id, f)| {
            (
                id.clone(),
                DealApplications {
                    days: f.days.clone(),
                    unknown_dates: f.unknown_dates,
                    live: f.live,
                },
            )
        })
        .collect();
    let value = Arc::new(Published { built_at, deals });
    if let Ok(mut slot) = CURRENT.write() {
        *slot = Some(value);
    }
}

/// いま公開されている応募日。まだ集計されていなければ None。
pub fn current() -> Option<Arc<Published>> {
    CURRENT.read().ok().and_then(|slot| slot.clone())
}
