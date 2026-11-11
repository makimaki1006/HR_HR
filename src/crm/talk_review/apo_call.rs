//! アポ数の取引ごとにランクを 1 つ決める (2026-11-10 ユーザー決定)。
//!
//! - 候補: 取引に付いた電話 ∪ 取引の担当者 (コンタクト) に付いた電話のうち、Zoom 純正連携の発信で 60 秒以上、
//!   アポ日確定に入った時刻 E の 24 時間前〜10 分後のもの。
//! - アポを取った電話: 候補のうち今の判定が「日程が決まった」電話で、E 以前で E に最も近いもの
//!   (無ければ E の後で最も近いもの)。無ければ「電話なし」(日程の変更・確認の電話しか無い取引も含む)。
//! - ヒアリング: アポを取った電話から E の 10 分後までの電話の答えを合わせる。後の電話の答えで上書きする。
//! - 候補の判定がそろうまで (または E の 36 時間後まで) は「判定待ち」。
//!
//! 結果はプロセス内だけに持つ (新しい表・列は作らない)。再起動したら作り直す。**単一インスタンス前提**。
//! 候補の電話に判定の行が無いとき (アポ取り以外として外した電話・対象チーム外の人の電話など) は、
//! 印 `apo_only` を付けた行を作って判定する。その行は 9 項目の割合・判定した電話数に入れない。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use chrono::NaiveDate;
use serde_json::{json, Value};

use super::counts::ApoDeal;
use super::judge::{StoredItem, SCHEDULED};
use super::store::{self, ReviewRow};
use super::worker::{parse_candidate, Candidate};
use crate::hubspot::{HubSpotClient, HubSpotError};

pub const APO_ONLY: &str = "apo_only";

/// アポのランクを決めるためだけに作った行か (9 項目の集計に入れない)
pub fn only(row: &ReviewRow) -> bool {
    row.note.as_deref() == Some(APO_ONLY)
}

const HOUR_MS: i64 = 3_600_000;
/// 候補の窓: E の 24 時間前から
pub const BEFORE_MS: i64 = 24 * HOUR_MS;
/// 候補の窓: E の 10 分後まで (ヒアリングもここまで)
pub const AFTER_MS: i64 = 600_000;
/// 候補にする電話の長さの下限
pub const MIN_DURATION_MS: i64 = 60_000;
/// 判定の行が無い電話を、取り込みの側で行になるのを待つ時間 (これを過ぎたら `apo_only` の行を作る)
pub const ROW_GRACE_MS: i64 = 3 * HOUR_MS;
/// 候補の判定がそろうのを待つ上限 (E から)。過ぎたら判定の済んだ電話だけで決める
pub const DECIDE_DEADLINE_MS: i64 = 36 * HOUR_MS;
/// ヒアリングがそろうのを待つ上限 (E から)。過ぎたら付いている分だけで決める
pub const HEARING_DEADLINE_MS: i64 = 48 * HOUR_MS;
/// 「電話なし」「取れない」と決めるまで待つ時間 (E から)。Call の作成・関連付けの遅れを待つ。この間は候補を読み直す
pub const SETTLE_WAIT_MS: i64 = 3 * HOUR_MS;
/// 候補を読み直す間隔 (`SETTLE_WAIT_MS` まで)
pub const REFETCH_EVERY_MS: i64 = 30 * 60_000;
/// 1 周に候補を読む取引の数
const FETCH_BATCH: usize = 50;

/// 取引 1 件のアポの記録 (同じ取引が別の日にまた入ったら別の記録)
pub type DealKey = (String, i64);

pub fn key(d: &ApoDeal) -> DealKey {
    (d.deal_id.clone(), d.entered_ms)
}

/// 窓に入る候補か (両端を含む)
pub fn in_window(deal: &ApoDeal, c: &Candidate) -> bool {
    c.duration_ms >= MIN_DURATION_MS
        && (deal.entered_ms - BEFORE_MS..=deal.entered_ms + AFTER_MS).contains(&c.ts_ms)
}

/// 取引の候補の電話 (窓の中だけ、古い順)
pub fn window<'a>(deal: &ApoDeal, calls: &'a [Candidate]) -> Vec<&'a Candidate> {
    let mut v: Vec<&Candidate> = calls.iter().filter(|c| in_window(deal, c)).collect();
    v.sort_by(|a, b| a.ts_ms.cmp(&b.ts_ms).then_with(|| a.id.cmp(&b.id)));
    v
}

/// 取引のアポを取った電話を決めた結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// まだ決められない (E の 10 分後より前・候補の判定待ち)
    Pending,
    /// 日程が決まった電話が無い
    NoCall,
    /// 日程が決まった電話は見つからず、録音が取れない候補があった
    Unavailable,
    /// アポを取った電話から E の 10 分後までの判定済みの電話 (古い順。先頭がアポを取った電話)
    Decided(Vec<String>),
}

fn terminal(status: &str) -> bool {
    matches!(
        status,
        store::ST_JUDGED | store::ST_UNAVAILABLE | store::ST_FAILED
    )
}

/// アポを取った電話を決める (純粋)。`rows` は候補の電話の判定の行 (無い電話は入れない)
pub fn pick(
    deal: &ApoDeal,
    calls: &[Candidate],
    rows: &HashMap<String, ReviewRow>,
    now_ms: i64,
) -> Pick {
    if now_ms < deal.entered_ms + AFTER_MS {
        return Pick::Pending;
    }
    let cands = window(deal, calls);
    let settled = cands
        .iter()
        .all(|c| rows.get(&c.id).is_some_and(|r| terminal(&r.status)));
    if !settled && now_ms < deal.entered_ms + DECIDE_DEADLINE_MS {
        return Pick::Pending;
    }
    let judged = |c: &Candidate| {
        rows.get(&c.id)
            .is_some_and(|r| r.status == store::ST_JUDGED)
    };
    let decided: Vec<&Candidate> = cands
        .iter()
        .copied()
        .filter(|c| judged(c))
        .filter(|c| rows[&c.id].outcome.as_deref() == Some(SCHEDULED))
        .collect();
    // 同じ近さなら、アポの付け先の人の電話・ID の順
    let credited = |c: &Candidate| deal.who.contains(&c.owner_id);
    let chosen = decided
        .iter()
        .filter(|c| c.ts_ms <= deal.entered_ms)
        .max_by(|a, b| {
            a.ts_ms
                .cmp(&b.ts_ms)
                .then_with(|| credited(a).cmp(&credited(b)))
                .then_with(|| b.id.cmp(&a.id))
        })
        .or_else(|| {
            decided
                .iter()
                .filter(|c| c.ts_ms > deal.entered_ms)
                .min_by(|a, b| {
                    a.ts_ms
                        .cmp(&b.ts_ms)
                        .then_with(|| credited(b).cmp(&credited(a)))
                        .then_with(|| a.id.cmp(&b.id))
                })
        });
    // 日程が決まった電話が無いと決めるのは、遅れて入る電話を待ってから
    if chosen.is_none() && now_ms < deal.entered_ms + SETTLE_WAIT_MS {
        return Pick::Pending;
    }
    match chosen {
        Some(first) => Pick::Decided(
            cands
                .iter()
                .filter(|c| c.id == first.id || (c.ts_ms > first.ts_ms && judged(c)))
                .map(|c| c.id.clone())
                .collect(),
        ),
        None if cands.iter().any(|c| {
            rows.get(&c.id).is_some_and(|r| {
                matches!(r.status.as_str(), store::ST_UNAVAILABLE | store::ST_FAILED)
            })
        }) =>
        {
            Pick::Unavailable
        }
        None => Pick::NoCall,
    }
}

/// 項目が答えを得られたか (`answered` が無い古い行は「できた」で答えがあるかで見る)
fn answered(i: &StoredItem) -> bool {
    i.answered
        .unwrap_or_else(|| i.status == "done" && i.answer.is_some())
}

/// 古い順の電話の項目から、ヒアリングの項目を合わせる (純粋)。
/// 項目ごとに、答えが得られた最後の電話の項目を使う。どの電話でも得られていなければ、該当なしでない最後の項目
pub fn merge_answers(per_call: &[&[StoredItem]], hearing_ids: &[String]) -> Vec<StoredItem> {
    hearing_ids
        .iter()
        .filter_map(|id| {
            let items: Vec<&StoredItem> = per_call
                .iter()
                .filter_map(|items| items.iter().find(|i| &i.id == id))
                .collect();
            items
                .iter()
                .rev()
                .find(|i| answered(i))
                .or_else(|| items.iter().rev().find(|i| i.status != "na"))
                .or_else(|| items.last())
                .map(|i| (*i).clone())
        })
        .collect()
}

/// 取引のランクの状態
#[derive(Debug, Clone, PartialEq)]
pub enum DealRank {
    Pending,
    NoCall,
    Unavailable,
    /// ランクが決まった。`call_id` はアポを取った電話、`calls` はヒアリングを合わせた電話、`items` は合わせたヒアリング
    Ranked {
        rank: &'static str,
        call_id: String,
        call_ms: i64,
        calls: Vec<String>,
        items: Vec<StoredItem>,
    },
}

impl DealRank {
    /// 決まった (これ以上変わらない) か
    pub fn settled(&self) -> bool {
        !matches!(self, DealRank::Pending)
    }
}

/// アポを取った電話とヒアリングから、取引のランクを決める (純粋)
pub fn deal_rank(
    deal: &ApoDeal,
    calls: &[Candidate],
    rows: &HashMap<String, ReviewRow>,
    has_hearing: impl Fn(&ReviewRow) -> bool,
    hearing_ids: &[String],
    now_ms: i64,
) -> DealRank {
    let ids = match pick(deal, calls, rows, now_ms) {
        Pick::Pending => return DealRank::Pending,
        Pick::NoCall => return DealRank::NoCall,
        Pick::Unavailable => return DealRank::Unavailable,
        Pick::Decided(ids) => ids,
    };
    let set: Vec<&ReviewRow> = ids.iter().filter_map(|id| rows.get(id)).collect();
    let heard: Vec<&ReviewRow> = set.iter().copied().filter(|r| has_hearing(r)).collect();
    if heard.len() < set.len() && now_ms < deal.entered_ms + HEARING_DEADLINE_MS {
        return DealRank::Pending;
    }
    let per_call: Vec<&[StoredItem]> = heard.iter().map(|r| r.items.as_slice()).collect();
    let items = merge_answers(&per_call, hearing_ids);
    let call_ms = calls
        .iter()
        .find(|c| c.id == ids[0])
        .map(|c| c.ts_ms)
        .unwrap_or(deal.entered_ms);
    match super::apo_rank::rank(&items) {
        Some(rank) => DealRank::Ranked {
            rank,
            call_id: ids[0].clone(),
            call_ms,
            calls: ids,
            items,
        },
        None => DealRank::Unavailable,
    }
}

#[derive(Default)]
struct Inner {
    /// 取引 → 候補の電話 (60 秒以上の Zoom 純正連携の発信。時刻で絞る前)。読んだ時刻も持つ
    calls: HashMap<String, (i64, Vec<Candidate>)>,
    /// アポの記録 → ランクの状態 (決まったものはもう計算しない)
    ranks: HashMap<DealKey, DealRank>,
    /// Slack に送った (または送らないと決めた) アポの記録
    notified: HashSet<DealKey>,
    /// この起動でヒアリングを書いた電話 (通知はこれを含む取引だけ。再起動で昔の分を送らない)
    heard_now: HashSet<String>,
    /// この起動で一度「判定待ち」だった記録 (起動して最初の評価で決まった記録は送らない)
    was_pending: HashSet<DealKey>,
    /// 画面で開かれた期間の最初の日のうち最も古い日 (worker はここからの取引も扱う)
    want_from: Option<NaiveDate>,
}

/// 取引ごとのアポを取った電話とランク (プロセス内だけ)
#[derive(Default)]
pub struct ApoCalls {
    inner: Mutex<Inner>,
}

impl ApoCalls {
    /// 集計用: アポの記録ごとのランクの状態 (無い記録は判定待ち)
    pub fn ranks(&self) -> HashMap<DealKey, DealRank> {
        self.inner.lock().unwrap().ranks.clone()
    }

    /// 画面で開かれた期間の最初の日を覚える。worker が直近 `CALL_REVIEW_BACKFILL_DAYS` 日より前の取引も扱うため
    /// (結果はプロセス内だけなので、再起動の後に古い期間を開くと、そこから作り直す)
    pub fn want(&self, from: NaiveDate) {
        let mut g = self.inner.lock().unwrap();
        g.want_from = Some(g.want_from.map_or(from, |w| w.min(from)));
    }

    /// worker が扱う取引の最初の日: 直近の日 (`recent`) と、画面で開かれた最も古い日の早い方
    pub fn scope_from(&self, recent: NaiveDate) -> NaiveDate {
        self.inner
            .lock()
            .unwrap()
            .want_from
            .map_or(recent, |w| w.min(recent))
    }

    /// ヒアリングを書いた電話を覚える (この電話を含む取引だけ、決まったときに通知する)
    pub fn mark_heard(&self, call_id: &str) {
        self.inner
            .lock()
            .unwrap()
            .heard_now
            .insert(call_id.to_string());
    }

    /// 候補を読む必要のある取引 (E の 10 分後を過ぎ、その後に読んでいない)。新しい記録から最大 `FETCH_BATCH` 件
    pub fn to_fetch(&self, deals: &[ApoDeal], now_ms: i64) -> Vec<String> {
        let g = self.inner.lock().unwrap();
        let mut due: Vec<&ApoDeal> = deals
            .iter()
            .filter(|d| now_ms >= d.entered_ms + AFTER_MS)
            .filter(|d| !g.ranks.get(&key(d)).is_some_and(DealRank::settled))
            .filter(|d| {
                g.calls.get(&d.deal_id).is_none_or(|(at, _)| {
                    *at < d.entered_ms + AFTER_MS
                        || (*at < d.entered_ms + SETTLE_WAIT_MS && now_ms - *at >= REFETCH_EVERY_MS)
                })
            })
            .collect();
        due.sort_by_key(|d| std::cmp::Reverse(d.entered_ms));
        let mut ids: Vec<String> = Vec::new();
        for d in due {
            if !ids.contains(&d.deal_id) {
                ids.push(d.deal_id.clone());
            }
            if ids.len() >= FETCH_BATCH {
                break;
            }
        }
        ids
    }

    pub fn store_calls(&self, read_ms: i64, found: HashMap<String, Vec<Candidate>>) {
        let mut g = self.inner.lock().unwrap();
        for (deal, calls) in found {
            g.calls.insert(deal, (read_ms, calls));
        }
    }

    /// まだ決まっていない記録と、その窓の候補 (E の 10 分後より後に候補を読んだ記録だけ)
    pub fn open(&self, deals: &[ApoDeal]) -> Vec<(ApoDeal, Vec<Candidate>)> {
        let g = self.inner.lock().unwrap();
        deals
            .iter()
            .filter(|d| !g.ranks.get(&key(d)).is_some_and(DealRank::settled))
            .filter_map(|d| {
                let (at, calls) = g.calls.get(&d.deal_id)?;
                (*at >= d.entered_ms + AFTER_MS)
                    .then(|| (d.clone(), window(d, calls).into_iter().cloned().collect()))
            })
            .collect()
    }

    /// ランクの状態を残す。送るのは次のすべてに当たるとき (true): ランクが決まった・この起動で一度判定待ちだった・
    /// ヒアリングを合わせた電話のどれかにこの起動でヒアリングを書いた・まだ送っていない。
    /// 再起動のあと、前の起動で送った取引を送り直さないため (送ったかは持ち越さない)
    pub fn record(&self, d: &ApoDeal, rank: DealRank) -> bool {
        let mut g = self.inner.lock().unwrap();
        let k = key(d);
        let notify = match &rank {
            DealRank::Ranked { calls, .. } => {
                g.was_pending.contains(&k)
                    && calls.iter().any(|id| g.heard_now.contains(id))
                    && !g.notified.contains(&k)
            }
            _ => false,
        };
        if rank.settled() {
            g.notified.insert(k.clone());
        } else {
            g.was_pending.insert(k.clone());
        }
        g.ranks.insert(k, rank);
        notify
    }
}

/// アポを取った電話とヒアリングを合わせる電話のうち、判定済みでヒアリングが付いていない電話 (純粋)。
/// ヒアリングを待つ上限 (E の 48 時間後) を過ぎた取引で、まだ付けていない電話にヒアリングを付けてから決めるために使う
/// (再起動の後の古い取引・#437 で新しく作った行など、ヒアリングの周回の一覧に入っていない電話)
pub fn missing_hearing(
    ids: &[String],
    rows: &HashMap<String, ReviewRow>,
    has_hearing: impl Fn(&ReviewRow) -> bool,
) -> Vec<String> {
    ids.iter()
        .filter(|id| {
            rows.get(*id)
                .is_some_and(|r| r.status == store::ST_JUDGED && !has_hearing(r))
        })
        .cloned()
        .collect()
}

/// 取引の候補の電話を HubSpot から読む (読み取りだけ、`Background`)。関連は取引の電話 ∪ 担当者の電話
pub async fn fetch_calls(
    client: &HubSpotClient,
    deal_ids: &[String],
) -> Result<HashMap<String, Vec<Candidate>>, HubSpotError> {
    let bg = client.background();
    let direct = bg.batch_associations("deals", "calls", deal_ids).await?;
    let contacts = bg.batch_associations("deals", "contacts", deal_ids).await?;
    let contact_ids: Vec<_> = contacts
        .values()
        .flatten()
        .map(|r| r.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let indirect = if contact_ids.is_empty() {
        Default::default()
    } else {
        bg.batch_associations("contacts", "calls", &contact_ids)
            .await?
    };
    let mut by_deal: HashMap<String, BTreeSet<String>> = HashMap::new();
    for id in deal_ids {
        let mut set: BTreeSet<String> = direct
            .get(id)
            .into_iter()
            .flatten()
            .map(|r| r.id.clone())
            .collect();
        for c in contacts.get(id).into_iter().flatten() {
            set.extend(
                indirect
                    .get(&c.id)
                    .into_iter()
                    .flatten()
                    .map(|r| r.id.clone()),
            );
        }
        by_deal.insert(id.clone(), set);
    }
    let call_ids: Vec<_> = by_deal
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let recs = if call_ids.is_empty() {
        vec![]
    } else {
        bg.batch_read(
            "calls",
            &call_ids,
            &[
                "hubspot_owner_id",
                "hs_timestamp",
                "hs_call_duration",
                "hs_call_direction",
                "hs_call_source",
                "hs_call_body",
                "hs_call_to_number",
            ],
        )
        .await?
    };
    let calls: HashMap<_, _> = recs
        .into_iter()
        .filter_map(|r| {
            let mut p: Value = serde_json::to_value(r.properties).ok()?;
            if p["hs_call_direction"] != "OUTBOUND"
                || p["hs_call_source"] != "INTEGRATIONS_PLATFORM"
            {
                return None;
            }
            // 担当者が空の電話も候補 (付け先はアポ数で決める)
            if p["hubspot_owner_id"]
                .as_str()
                .is_none_or(|s| s.trim().is_empty())
            {
                p["hubspot_owner_id"] = json!("unassigned");
            }
            let c = parse_candidate(&json!({"id": r.id, "properties": p}))?;
            (c.duration_ms >= MIN_DURATION_MS).then(|| (c.id.clone(), c))
        })
        .collect();
    Ok(by_deal
        .into_iter()
        .map(|(deal, ids)| {
            let v = ids.iter().filter_map(|id| calls.get(id).cloned()).collect();
            (deal, v)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: i64 = 100 * HOUR_MS;
    const DONE: i64 = E + DECIDE_DEADLINE_MS - 1;

    fn call(id: &str, offset: i64, owner: &str) -> Candidate {
        Candidate {
            id: id.into(),
            owner_id: owner.into(),
            ts_ms: E + offset,
            duration_ms: 90_000,
            zoom_id: None,
            to_number: None,
        }
    }
    fn deal() -> ApoDeal {
        ApoDeal {
            deal_id: "deal-test".into(),
            stage: "apo".into(),
            entered_ms: E,
            who: vec!["taker".into()],
        }
    }
    fn item(id: &str, status: &str, answer: Option<&str>, answered: Option<bool>) -> StoredItem {
        StoredItem {
            id: id.into(),
            status: status.into(),
            evidence: vec![],
            quote: None,
            advice: None,
            note: None,
            answer: answer.map(Into::into),
            asked: None,
            answered,
            counted_from: None,
        }
    }
    fn row(id: &str, status: &str, outcome: Option<&str>, items: Vec<StoredItem>) -> ReviewRow {
        ReviewRow {
            hubspot_call_id: id.into(),
            owner_id: "caller".into(),
            call_at: "2026-11-01T00:00:00Z".into(),
            duration_sec: 90,
            zoom_call_log_id: None,
            status: status.into(),
            attempts: 0,
            checks_version: None,
            model: None,
            outcome: outcome.map(Into::into),
            reached_person: None,
            reception: None,
            first_date_turn: None,
            items,
            summary: None,
            note: None,
        }
    }
    fn rows(v: Vec<ReviewRow>) -> HashMap<String, ReviewRow> {
        v.into_iter()
            .map(|r| (r.hubspot_call_id.clone(), r))
            .collect()
    }
    fn ids(v: &[&str]) -> Pick {
        Pick::Decided(v.iter().map(|s| s.to_string()).collect())
    }
    fn hearing_ids() -> Vec<String> {
        vec!["aq_budget".into(), "aq_timing".into(), "aq_decider".into()]
    }
    fn heard(r: &ReviewRow) -> bool {
        r.items.iter().any(|i| i.id.starts_with("aq_"))
    }

    #[test]
    fn window_edges_and_duration() {
        for (offset, duration, inside) in [
            (-BEFORE_MS, 60_000, true),
            (-BEFORE_MS - 1, 60_000, false),
            (AFTER_MS, 60_000, true),
            (AFTER_MS + 1, 60_000, false),
            (0, 59_999, false),
        ] {
            let mut c = call("c", offset, "x");
            c.duration_ms = duration;
            assert_eq!(in_window(&deal(), &c), inside, "{offset} {duration}");
        }
    }

    #[test]
    fn the_scheduled_call_wins_over_a_nearer_follow_up() {
        // 日程を決めた電話 (-3 時間) の後に、追加の電話 (-10 分) がある
        let calls = [
            call("decided", -3 * HOUR_MS, "taker"),
            call("follow", -600_000, "taker"),
        ];
        let r = rows(vec![
            row("decided", store::ST_JUDGED, Some(SCHEDULED), vec![]),
            row("follow", store::ST_JUDGED, Some("決まらなかった"), vec![]),
        ]);
        assert_eq!(pick(&deal(), &calls, &r, DONE), ids(&["decided", "follow"]));
        // 前日の電話 (-20 時間) でも窓の中なら選ぶ (3 時間の窓では取りこぼしていた)
        let calls = [call("day-before", -20 * HOUR_MS, "taker")];
        let r = rows(vec![row(
            "day-before",
            store::ST_JUDGED,
            Some(SCHEDULED),
            vec![],
        )]);
        assert_eq!(pick(&deal(), &calls, &r, DONE), ids(&["day-before"]));
    }

    #[test]
    fn closest_before_then_after_and_ties_go_to_the_taker() {
        let r = |v: &[&str]| {
            rows(
                v.iter()
                    .map(|id| row(id, store::ST_JUDGED, Some(SCHEDULED), vec![]))
                    .collect(),
            )
        };
        let calls = [
            call("older", -HOUR_MS, "x"),
            call("before", -60_000, "x"),
            call("after", 1, "x"),
        ];
        assert_eq!(
            pick(&deal(), &calls, &r(&["older", "before", "after"]), DONE),
            ids(&["before", "after"])
        );
        assert_eq!(
            pick(&deal(), &calls[2..], &r(&["after"]), DONE),
            ids(&["after"])
        );
        let calls = [call("a", -60_000, "other"), call("b", -60_000, "taker")];
        assert_eq!(pick(&deal(), &calls, &r(&["a", "b"]), DONE), ids(&["b"]));
    }

    #[test]
    fn waits_until_ten_minutes_after_and_until_candidates_are_judged() {
        let calls = [call("c", -60_000, "taker")];
        let judged = rows(vec![row("c", store::ST_JUDGED, Some(SCHEDULED), vec![])]);
        assert_eq!(
            pick(&deal(), &calls, &judged, E + AFTER_MS - 1),
            Pick::Pending
        );
        assert_eq!(pick(&deal(), &calls, &judged, E + AFTER_MS), ids(&["c"]));
        let waiting = rows(vec![row("c", store::ST_PENDING, None, vec![])]);
        assert_eq!(pick(&deal(), &calls, &waiting, DONE), Pick::Pending);
        // 行がまだ無い候補も待つ
        assert_eq!(pick(&deal(), &calls, &HashMap::new(), DONE), Pick::Pending);
        // 待つ上限を過ぎたら、判定の済んだ電話だけで決める
        assert_eq!(
            pick(&deal(), &calls, &waiting, E + DECIDE_DEADLINE_MS),
            Pick::NoCall
        );
    }

    #[test]
    fn no_scheduled_call_is_no_call_or_unavailable() {
        assert_eq!(pick(&deal(), &[], &HashMap::new(), DONE), Pick::NoCall);
        // 遅れて入る電話を待つ間は決めない
        assert_eq!(
            pick(&deal(), &[], &HashMap::new(), E + SETTLE_WAIT_MS - 1),
            Pick::Pending
        );
        // 日程の確認・変更の電話だけ (今の判定が「日程が決まった」でない) は電話なし
        let calls = [call("confirm", -HOUR_MS, "taker")];
        let r = rows(vec![row(
            "confirm",
            store::ST_JUDGED,
            Some("決まらなかった"),
            vec![],
        )]);
        assert_eq!(pick(&deal(), &calls, &r, DONE), Pick::NoCall);
        let r = rows(vec![row("confirm", store::ST_UNAVAILABLE, None, vec![])]);
        assert_eq!(pick(&deal(), &calls, &r, DONE), Pick::Unavailable);
    }

    #[test]
    fn later_answers_override_earlier_ones() {
        let first = vec![
            item("aq_budget", "not_done", Some("不明"), Some(false)),
            item("aq_timing", "done", Some("3か月以内"), Some(true)),
            item("aq_decider", "done", Some("本人以外が判断者"), Some(true)),
        ];
        let second = vec![
            item("aq_budget", "done", Some("具体額"), Some(true)),
            item("aq_timing", "not_done", Some("不明"), Some(false)),
            item("aq_decider", "done", Some("本人が判断者"), Some(true)),
        ];
        let m = merge_answers(&[&first, &second], &hearing_ids());
        let ans: Vec<_> = m.iter().map(|i| i.answer.as_deref().unwrap()).collect();
        // 予算は後で得られた答え、時期は後で得られなかったので前の答え、判断者は後の答え
        assert_eq!(ans, ["具体額", "3か月以内", "本人が判断者"]);
        assert_eq!(super::super::apo_rank::rank(&m), Some("S"));
    }

    #[test]
    fn rank_combines_hearing_across_the_calls_and_waits_for_each() {
        let calls = [
            call("decided", -2 * HOUR_MS, "taker"),
            call("follow", -HOUR_MS, "taker"),
        ];
        let decided_items = vec![
            item("aq_budget", "not_done", Some("不明"), Some(false)),
            item("aq_timing", "done", Some("3か月以内"), Some(true)),
            item("aq_decider", "done", Some("本人が判断者"), Some(true)),
        ];
        let mut r = rows(vec![
            row("decided", store::ST_JUDGED, Some(SCHEDULED), decided_items),
            row("follow", store::ST_JUDGED, Some("決まらなかった"), vec![]),
        ]);
        // 追加の電話のヒアリングがまだ: 待つ
        assert_eq!(
            deal_rank(&deal(), &calls, &r, heard, &hearing_ids(), E + HOUR_MS),
            DealRank::Pending
        );
        // 待つ上限を過ぎたら、付いている分だけで決める (予算が不明のままで A)
        let DealRank::Ranked { rank, call_id, .. } = deal_rank(
            &deal(),
            &calls,
            &r,
            heard,
            &hearing_ids(),
            E + HEARING_DEADLINE_MS,
        ) else {
            panic!("ranked")
        };
        assert_eq!((rank, call_id.as_str()), ("A", "decided"));
        // 追加の電話で予算が聞けたら S
        r.get_mut("follow").unwrap().items =
            vec![item("aq_budget", "done", Some("具体額"), Some(true))];
        let DealRank::Ranked { rank, .. } =
            deal_rank(&deal(), &calls, &r, heard, &hearing_ids(), E + HOUR_MS)
        else {
            panic!("ranked")
        };
        assert_eq!(rank, "S");
    }

    #[test]
    fn notice_only_once_and_only_for_hearing_written_in_this_run() {
        let cache = ApoCalls::default();
        let ranked = |calls: &[&str]| DealRank::Ranked {
            rank: "S",
            call_id: "c".into(),
            call_ms: E,
            calls: calls.iter().map(|s| s.to_string()).collect(),
            items: vec![],
        };
        // 再起動のあと最初の評価で決まった取引は、この起動でヒアリングを書いていても送らない
        cache.mark_heard("c");
        assert!(!cache.record(&deal(), ranked(&["c"])));
        // 判定待ちだった取引でも、ヒアリングを合わせた電話の外 (前の候補) に書いただけなら送らない
        let mut other = deal();
        other.entered_ms += 2;
        cache.mark_heard("earlier");
        assert!(!cache.record(&other, DealRank::Pending));
        assert!(!cache.record(&other, ranked(&["d"])));
        // 判定待ち → ヒアリングを書いて決まった: 1 回だけ送る
        let mut later = deal();
        later.entered_ms += 1;
        assert!(!cache.record(&later, DealRank::Pending));
        assert!(cache.record(&later, ranked(&["c"])));
        assert!(!cache.record(&later, ranked(&["c"])), "一度だけ");
        // 決まった記録はもう開かない・読み直さない
        cache.store_calls(
            E + SETTLE_WAIT_MS,
            HashMap::from([("deal-test".into(), vec![])]),
        );
        assert!(cache
            .open(&[deal(), later.clone(), other.clone()])
            .is_empty());
        assert!(cache.to_fetch(&[deal(), later, other], E + DONE).is_empty());
    }

    #[test]
    fn candidates_are_read_after_ten_minutes_and_reread_for_a_later_record() {
        let cache = ApoCalls::default();
        assert!(cache.to_fetch(&[deal()], E + AFTER_MS - 1).is_empty());
        assert_eq!(cache.to_fetch(&[deal()], E + AFTER_MS), ["deal-test"]);
        cache.store_calls(E + AFTER_MS, HashMap::from([("deal-test".into(), vec![])]));
        assert!(cache
            .to_fetch(&[deal()], E + AFTER_MS + REFETCH_EVERY_MS - 1)
            .is_empty());
        assert_eq!(cache.open(&[deal()]).len(), 1);
        // 遅れて入る電話のために、3 時間までは 30 分おきに読み直す
        assert_eq!(
            cache.to_fetch(&[deal()], E + AFTER_MS + REFETCH_EVERY_MS),
            ["deal-test"]
        );
        cache.store_calls(
            E + SETTLE_WAIT_MS,
            HashMap::from([("deal-test".into(), vec![])]),
        );
        assert!(cache.to_fetch(&[deal()], E + 10 * HOUR_MS).is_empty());
        // 同じ取引が翌日にまた入ったら、その後で読み直す
        let mut again = deal();
        again.entered_ms = E + 24 * HOUR_MS;
        assert_eq!(
            cache.to_fetch(&[deal(), again.clone()], again.entered_ms + AFTER_MS),
            ["deal-test"]
        );
        assert_eq!(cache.open(&[again]).len(), 0, "読む前は開かない");
    }
    #[test]
    fn missing_hearing_only_judged_rows_without_hearing() {
        let with = row(
            "a",
            store::ST_JUDGED,
            Some(SCHEDULED),
            vec![item("aq_budget", "done", Some("具体額"), Some(true))],
        );
        let without = row("b", store::ST_JUDGED, Some(SCHEDULED), vec![]);
        let pending = row("c", store::ST_PENDING, None, vec![]);
        let r = rows(vec![with, without, pending]);
        let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(missing_hearing(&ids, &r, heard), vec!["b".to_string()]);
    }

    #[test]
    fn scope_from_takes_the_oldest_wanted_day() {
        let day = |d: u32| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        let a = ApoCalls::default();
        assert_eq!(a.scope_from(day(1)), day(1));
        a.want(day(5));
        assert_eq!(a.scope_from(day(1)), day(1));
        a.want(day(3));
        a.want(day(9));
        assert_eq!(a.scope_from(day(10)), day(3));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::hubspot::ClientOptions;
    use axum::{
        extract::{Path, State},
        routing::post,
        Json, Router,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    async fn associations(
        State(n): State<Arc<AtomicUsize>>,
        Path((from, to)): Path<(String, String)>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        n.fetch_add(1, Ordering::SeqCst);
        let results: Vec<_> = body["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                let id = i["id"].as_str().unwrap();
                let targets = match (from.as_str(), to.as_str(), id) {
                    ("deals", "calls", "801") => vec!["901"],
                    ("deals", "contacts", "801") => vec!["701"],
                    ("contacts", "calls", "701") => vec!["901", "902", "903", "904", "905"],
                    _ => vec![],
                };
                let to: Vec<_> = targets
                    .iter()
                    .map(|t| json!({"toObjectId": t, "associationTypes": []}))
                    .collect();
                json!({"from": {"id": id}, "to": to})
            })
            .collect();
        Json(json!({ "results": results }))
    }
    async fn calls(State(n): State<Arc<AtomicUsize>>, Json(body): Json<Value>) -> Json<Value> {
        n.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            body["inputs"].as_array().unwrap().len(),
            5,
            "関連の重複を外す"
        );
        let results: Vec<_> = [
            ("901", "OUTBOUND", "INTEGRATIONS_PLATFORM", 90_000),
            ("902", "OUTBOUND", "INTEGRATIONS_PLATFORM", 60_000),
            ("903", "INBOUND", "INTEGRATIONS_PLATFORM", 90_000),
            ("904", "OUTBOUND", "OTHER", 90_000),
            ("905", "OUTBOUND", "INTEGRATIONS_PLATFORM", 59_999),
        ]
        .iter()
        .map(|(id, direction, source, duration)| {
            json!({"id": id, "properties": {
                "hs_timestamp": "20000000", "hubspot_owner_id": "",
                "hs_call_duration": duration.to_string(),
                "hs_call_direction": direction, "hs_call_source": source}})
        })
        .collect();
        Json(json!({ "results": results }))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn union_of_deal_and_contact_calls_keeps_only_long_platform_outbound() {
        let n = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/crm/v4/associations/{from}/{to}/batch/read",
                post(associations),
            )
            .route("/crm/v3/objects/calls/batch/read", post(calls))
            .with_state(n.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client =
            HubSpotClient::new("fake-test-key".into(), &url, ClientOptions::default()).unwrap();
        let found = fetch_calls(&client, &["801".into(), "802".into()])
            .await
            .unwrap();
        let mut got: Vec<_> = found["801"].iter().map(|c| c.id.as_str()).collect();
        got.sort();
        assert_eq!(got, ["901", "902"]);
        assert!(found["801"].iter().all(|c| c.owner_id == "unassigned"));
        assert!(found["802"].is_empty());
        assert_eq!(n.load(Ordering::SeqCst), 4);
    }
}
