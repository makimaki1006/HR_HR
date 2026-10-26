//! 9 項目の判定の対象 = 接触の判定で `contacted = 1` になった電話 (2026-10-26、ADR-033)。
//!
//! 接触の判定 (`call_contact_marks`、1 行 = Zoom の通話ログ 1 本) と HubSpot の Call (`call_talk_reviews`) を結び付ける。
//! - Call の本文に録音 URL があれば、その Zoom の通話ログ ID と同じ印が相手。
//! - URL が無い Call (Call Queue 経由) は、同じ担当者で時刻が近い (3 分以内) 印のうち最も近いもの。
//!
//! 接触していない電話は行を作らない (Turso の書き込みを増やさない)。ここは純粋な部分だけ。

use std::collections::HashSet;

use chrono::DateTime;

use super::store::ReviewRow;
use super::zoom::{CALLER_WINDOW_MS, REC_PREFIX};

/// 接触した電話として行を作るときに `note` に入れる印。順番待ちの行を枠に入れるのはこの印のある行だけ
/// (この変更より前の、接触を調べていない行を枠に入れて判定しないため)
pub const CONTACTED_NOTE: &str = "contacted";

/// 印が付くまで待つ上限 (通話ログに録音の印がまだ無い間)
pub const NO_MARK_WAIT: chrono::Duration = chrono::Duration::hours(6);
/// 接触の判定中の印を待つ上限 (判定の再試行は約 28 時間続く)
pub const JUDGING_WAIT: chrono::Duration = chrono::Duration::hours(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkState {
    /// Zoom の通話ログ ID
    pub id: String,
    pub owner_id: String,
    pub at_ms: i64,
    /// `pending` / `judged` / `unavailable`
    pub status: String,
    pub contacted: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitKind {
    /// 印がまだ無い
    NoMark,
    /// 印はあるが接触の判定が済んでいない
    Judging,
}

impl WaitKind {
    pub fn limit(self) -> chrono::Duration {
        match self {
            WaitKind::NoMark => NO_MARK_WAIT,
            WaitKind::Judging => JUDGING_WAIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 接触した電話 (対応する印の ID)
    Target(String),
    /// 接触していない・判定できない (9 項目の対象にしない)
    Skip,
    Wait(WaitKind),
}

/// Call に対応する印。`link` (録音 URL の通話ログ ID) があればその ID、無ければ同じ担当者で最も近い未使用の印
fn find<'a>(
    link: Option<&str>,
    owner: &str,
    at_ms: i64,
    marks: &'a [MarkState],
    used: &HashSet<String>,
) -> Option<&'a MarkState> {
    match link {
        Some(id) => marks.iter().find(|m| m.id == id),
        None => marks
            .iter()
            .filter(|m| {
                m.owner_id == owner
                    && !used.contains(&m.id)
                    && (m.at_ms - at_ms).abs() <= CALLER_WINDOW_MS
            })
            .min_by_key(|m| (m.at_ms - at_ms).abs()),
    }
}

/// 取り込み前の Call について、9 項目の対象にするか待つか見送るかを決める
pub fn verdict(
    link: Option<&str>,
    owner: &str,
    at_ms: i64,
    marks: &[MarkState],
    used: &mut HashSet<String>,
) -> Verdict {
    let Some(m) = find(link, owner, at_ms, marks, used) else {
        return Verdict::Wait(WaitKind::NoMark);
    };
    match (m.status.as_str(), m.contacted) {
        ("pending", _) => Verdict::Wait(WaitKind::Judging),
        ("judged", Some(true)) => {
            used.insert(m.id.clone());
            Verdict::Target(m.id.clone())
        }
        _ => {
            used.insert(m.id.clone());
            Verdict::Skip
        }
    }
}

/// 録音 URL から取った通話ログ ID。Zoom の通話履歴から見つけた録音 ID (`rec:`) は通話ログ ID ではない
pub fn link_of(r: &ReviewRow) -> Option<&str> {
    r.zoom_call_log_id
        .as_deref()
        .filter(|z| !z.starts_with(REC_PREFIX))
}

/// 集計・一覧に入れる行 = 接触した電話。`marks` は接触した印 (`contacted = 1`) だけでよい。
/// 録音 URL のある行はその ID の印があるものだけ、無い行は同じ担当者で最も近い未使用の印があるものだけを残す。
/// 並びは入力のまま
pub fn contacted_rows(rows: Vec<ReviewRow>, marks: &[MarkState]) -> Vec<ReviewRow> {
    let mut used: HashSet<String> = HashSet::new();
    let mut keep = vec![false; rows.len()];
    // 録音 URL のある行を先に確定し、その印を時刻の照合から外す
    for (i, r) in rows.iter().enumerate() {
        if let Some(id) = link_of(r) {
            if marks.iter().any(|m| m.id == id) {
                keep[i] = true;
                used.insert(id.to_string());
            }
        }
    }
    for (i, r) in rows.iter().enumerate() {
        if link_of(r).is_some() {
            continue;
        }
        let Ok(at) = DateTime::parse_from_rfc3339(&r.call_at) else {
            continue;
        };
        if let Some(m) = find(None, &r.owner_id, at.timestamp_millis(), marks, &used) {
            used.insert(m.id.clone());
            keep[i] = true;
        }
    }
    rows.into_iter()
        .zip(keep)
        .filter_map(|(r, k)| k.then_some(r))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::store::ST_JUDGED;

    fn ms(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }
    fn mark(id: &str, owner: &str, at: &str, status: &str, c: Option<bool>) -> MarkState {
        MarkState {
            id: id.into(),
            owner_id: owner.into(),
            at_ms: ms(at),
            status: status.into(),
            contacted: c,
        }
    }
    fn row(id: &str, owner: &str, at: &str, zid: Option<&str>) -> ReviewRow {
        ReviewRow {
            hubspot_call_id: id.into(),
            owner_id: owner.into(),
            call_at: at.into(),
            duration_sec: 40,
            zoom_call_log_id: zid.map(String::from),
            status: ST_JUDGED.into(),
            attempts: 1,
            checks_version: None,
            model: None,
            outcome: None,
            reached_person: None,
            reception: None,
            first_date_turn: None,
            items: vec![],
            summary: None,
            note: None,
        }
    }

    #[test]
    fn url_calls_follow_the_mark_with_the_same_zoom_id() {
        let marks = vec![
            mark("Z1", "o1", "2026-10-26T01:00:00Z", "judged", Some(true)),
            mark("Z2", "o1", "2026-10-26T01:05:00Z", "judged", Some(false)),
            mark("Z3", "o1", "2026-10-26T01:10:00Z", "pending", None),
            mark("Z4", "o1", "2026-10-26T01:15:00Z", "unavailable", None),
        ];
        let mut used = HashSet::new();
        let v = |id: &str, used: &mut HashSet<String>| {
            verdict(Some(id), "o1", ms("2026-10-26T01:00:00Z"), &marks, used)
        };
        assert_eq!(v("Z1", &mut used), Verdict::Target("Z1".into()));
        assert_eq!(v("Z2", &mut used), Verdict::Skip, "接触していない");
        assert_eq!(v("Z3", &mut used), Verdict::Wait(WaitKind::Judging));
        assert_eq!(
            v("Z4", &mut used),
            Verdict::Skip,
            "取れない印は対象にしない"
        );
        assert_eq!(v("Z9", &mut used), Verdict::Wait(WaitKind::NoMark));
    }

    #[test]
    fn calls_without_url_follow_the_nearest_unused_mark_of_the_same_owner() {
        let marks = vec![
            mark("A", "o1", "2026-10-26T01:00:10Z", "judged", Some(true)),
            mark("B", "o1", "2026-10-26T01:01:30Z", "judged", Some(false)),
            mark("C", "o2", "2026-10-26T01:00:00Z", "judged", Some(true)),
        ];
        let mut used = HashSet::new();
        // 10 秒差の A (接触) が 90 秒差の B より近い
        assert_eq!(
            verdict(None, "o1", ms("2026-10-26T01:00:00Z"), &marks, &mut used),
            Verdict::Target("A".into())
        );
        // A は使用済みなので、同じ頃の次の Call は B (接触していない) に当たる
        assert_eq!(
            verdict(None, "o1", ms("2026-10-26T01:00:05Z"), &marks, &mut used),
            Verdict::Skip
        );
        // もう未使用の印が無い
        assert_eq!(
            verdict(None, "o1", ms("2026-10-26T01:00:00Z"), &marks, &mut used),
            Verdict::Wait(WaitKind::NoMark)
        );
        // 別の担当者の印・3 分を超える印には結び付かない
        assert_eq!(
            verdict(None, "o3", ms("2026-10-26T01:00:00Z"), &marks, &mut used),
            Verdict::Wait(WaitKind::NoMark)
        );
        assert_eq!(
            verdict(None, "o2", ms("2026-10-26T01:03:01Z"), &marks, &mut used),
            Verdict::Wait(WaitKind::NoMark)
        );
        assert_eq!(
            verdict(None, "o2", ms("2026-10-26T01:03:00Z"), &marks, &mut used),
            Verdict::Target("C".into())
        );
    }

    #[test]
    fn summary_keeps_only_rows_that_match_a_contacted_mark() {
        let marks = vec![
            mark("Z1", "o1", "2026-10-26T01:00:00Z", "judged", Some(true)),
            mark("Q1", "o2", "2026-10-26T02:00:30Z", "judged", Some(true)),
        ];
        let rows = vec![
            // 録音 URL があり、接触した印がある
            row("h1", "o1", "2026-10-26T01:00:00Z", Some("Z1")),
            // 録音 URL があるが、接触した印が無い (接触していない・調べていない)
            row("h2", "o1", "2026-10-26T01:30:00Z", Some("Z2")),
            // Call Queue 経由 (rec: の録音 ID) で、同じ担当者・30 秒差の接触した印がある
            row("h3", "o2", "2026-10-26T02:00:00Z", Some("rec:R9")),
            // 録音の手掛かりが無く、近い印も無い
            row("h4", "o2", "2026-10-26T05:00:00Z", None),
            // 担当者が違えば時刻が合っても結び付かない
            row("h5", "o9", "2026-10-26T02:00:00Z", None),
        ];
        let kept = contacted_rows(rows, &marks);
        assert_eq!(
            kept.iter()
                .map(|r| r.hubspot_call_id.as_str())
                .collect::<Vec<_>>(),
            vec!["h1", "h3"]
        );
    }

    #[test]
    fn a_url_call_claims_its_mark_before_a_url_less_call_nearby() {
        let marks = vec![mark(
            "Z1",
            "o1",
            "2026-10-26T01:00:00Z",
            "judged",
            Some(true),
        )];
        let rows = vec![
            row("h0", "o1", "2026-10-26T01:00:20Z", None),
            row("h1", "o1", "2026-10-26T01:00:00Z", Some("Z1")),
        ];
        let kept = contacted_rows(rows, &marks);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].hubspot_call_id, "h1");
    }
}
