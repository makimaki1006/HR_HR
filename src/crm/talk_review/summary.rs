//! 人ごとの表と印 (純粋な計算)。

use std::collections::{BTreeSet, HashMap};

use super::store::{ReviewRow, ST_FAILED, ST_JUDGED, ST_PENDING, ST_UNAVAILABLE};
use super::types::{TalkAlertRules, TalkItemRate, TalkPersonRow};

/// 印の決まり (1 か所。画面の下の説明にもこの値が出る)
pub const ALERT_RULES: TalkAlertRules = TalkAlertRules {
    item_min_applicable: 5,
    item_excess_pct: 20.0,
    appointment_min_dials: 100,
    appointment_rate_ratio: 0.5,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub owner_id: String,
    pub name: String,
    pub team: Option<String>,
}

pub fn pct(num: u32, den: u32) -> Option<f64> {
    (den > 0).then(|| f64::from(num) / f64::from(den) * 100.0)
}

fn item_rates(rows: &[&ReviewRow], ids: &[String]) -> Vec<TalkItemRate> {
    ids.iter()
        .map(|id| {
            let (mut nd, mut app) = (0u32, 0u32);
            for r in rows.iter().filter(|r| r.status == ST_JUDGED) {
                match r
                    .items
                    .iter()
                    .find(|i| &i.id == id)
                    .map(|i| i.status.as_str())
                {
                    Some("not_done") => {
                        nd += 1;
                        app += 1;
                    }
                    Some("done") => app += 1,
                    _ => {}
                }
            }
            TalkItemRate {
                id: id.clone(),
                not_done: nd,
                applicable: app,
                rate_pct: pct(nd, app),
                flagged: false,
            }
        })
        .collect()
}

fn counts(rows: &[&ReviewRow]) -> (u32, u32, u32) {
    let c = |f: &dyn Fn(&str) -> bool| rows.iter().filter(|r| f(&r.status)).count() as u32;
    (
        c(&|s| s == ST_JUDGED),
        c(&|s| s == ST_PENDING),
        c(&|s| s == ST_UNAVAILABLE || s == ST_FAILED),
    )
}

/// 対象の担当者のだれかに付いた取引の数 (取引ごとに 1)
pub fn unique_deals(deals: &[Vec<String>], owners: &BTreeSet<&str>) -> u32 {
    deals
        .iter()
        .filter(|d| d.iter().any(|o| owners.contains(o.as_str())))
        .count() as u32
}

/// 表の行と、チーム全体の行。`members` は絞り込み後の人。
pub fn build(
    members: &[Member],
    dials: Option<&HashMap<String, u32>>,
    outbound: Option<&HashMap<String, u32>>,
    apo_deals: &[Vec<String>],
    rows: &[ReviewRow],
    check_ids: &[String],
    rules: &TalkAlertRules,
) -> (Vec<TalkPersonRow>, TalkPersonRow) {
    let mut people: Vec<TalkPersonRow> = Vec::new();
    // 人ごとのアポ数 (取引に付いた担当者ごとに 1)
    let mut appointments: HashMap<&str, u32> = HashMap::new();
    for deal in apo_deals {
        for o in deal {
            *appointments.entry(o.as_str()).or_default() += 1;
        }
    }
    for m in members {
        let mine: Vec<&ReviewRow> = rows.iter().filter(|r| r.owner_id == m.owner_id).collect();
        // シートが読めたなら、行の無い人は 0 本
        let d = dials.map(|m2| m2.get(&m.owner_id).copied().unwrap_or(0));
        let ob = outbound.map(|m2| m2.get(&m.owner_id).copied().unwrap_or(0));
        let a = appointments.get(m.owner_id.as_str()).copied().unwrap_or(0);
        if d.unwrap_or(0) == 0 && a == 0 && mine.is_empty() {
            continue;
        }
        let (reviewed, waiting, unavailable) = counts(&mine);
        people.push(TalkPersonRow {
            owner_id: Some(m.owner_id.clone()),
            name: m.name.clone(),
            team: m.team.clone(),
            outbound: ob,
            dials: d,
            appointments: a,
            appointment_rate_pct: d.and_then(|d| pct(a, d)),
            appointment_rate_flagged: false,
            contacts: None,
            contact_rate_pct: None,
            date_proposal_calls: None,
            date_proposal_rate_pct: None,
            proposals_per_contact: None,
            contact_pending: 0,
            eligible: mine.len() as u32,
            reviewed,
            waiting,
            unavailable,
            items: item_rates(&mine, check_ids),
        });
    }
    people.sort_by(|a, b| b.dials.cmp(&a.dials).then_with(|| a.name.cmp(&b.name)));

    let ids: BTreeSet<&str> = people
        .iter()
        .filter_map(|p| p.owner_id.as_deref())
        .collect();
    let all: Vec<&ReviewRow> = rows
        .iter()
        .filter(|r| ids.contains(r.owner_id.as_str()))
        .collect();
    let (reviewed, waiting, unavailable) = counts(&all);
    let dials_sum: Option<u32> = dials.map(|_| people.iter().map(|p| p.dials.unwrap_or(0)).sum());
    // チーム全体は取引を一意にして数える (BPO と担当の新規営業の両方に付いた取引を 2 回数えない)
    let outbound_sum: Option<u32> =
        outbound.map(|_| people.iter().map(|p| p.outbound.unwrap_or(0)).sum());
    let apo_sum = unique_deals(apo_deals, &ids);
    let total = TalkPersonRow {
        owner_id: None,
        name: "チーム全体".to_string(),
        team: None,
        outbound: outbound_sum,
        dials: dials_sum,
        appointments: apo_sum,
        appointment_rate_pct: dials_sum.and_then(|d| pct(apo_sum, d)),
        appointment_rate_flagged: false,
        contacts: None,
        contact_rate_pct: None,
        date_proposal_calls: None,
        date_proposal_rate_pct: None,
        proposals_per_contact: None,
        contact_pending: 0,
        eligible: all.len() as u32,
        reviewed,
        waiting,
        unavailable,
        items: item_rates(&all, check_ids),
    };
    for p in &mut people {
        p.appointment_rate_flagged = match (p.appointment_rate_pct, total.appointment_rate_pct) {
            (Some(mine), Some(team)) => {
                p.dials.is_some_and(|d| d >= rules.appointment_min_dials)
                    && mine < team * rules.appointment_rate_ratio
            }
            _ => false,
        };
        for (it, t) in p.items.iter_mut().zip(&total.items) {
            it.flagged = match (it.rate_pct, t.rate_pct) {
                (Some(mine), Some(team)) => {
                    it.applicable >= rules.item_min_applicable
                        && mine - team >= rules.item_excess_pct
                }
                _ => false,
            };
        }
    }
    (people, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::judge::StoredItem;

    fn row(id: &str, owner: &str, status: &str, early: Option<&str>) -> ReviewRow {
        ReviewRow {
            hubspot_call_id: id.into(),
            owner_id: owner.into(),
            call_at: "2026-10-15T01:00:00Z".into(),
            duration_sec: 100,
            zoom_call_log_id: None,
            status: status.into(),
            attempts: 1,
            checks_version: None,
            model: None,
            outcome: None,
            reached_person: None,
            reception: None,
            first_date_turn: None,
            items: early
                .map(|s| {
                    vec![StoredItem {
                        id: "early_date".into(),
                        status: s.into(),
                        evidence: vec![],
                        quote: None,
                        advice: None,
                        note: None,
                    }]
                })
                .unwrap_or_default(),
            summary: None,
            note: None,
        }
    }

    fn m(id: &str, name: &str) -> Member {
        Member {
            owner_id: id.into(),
            name: name.into(),
            team: Some("新規営業".into()),
        }
    }

    #[test]
    fn flags_item_when_five_or_more_and_twenty_points_above_team() {
        // A: 該当 5 本のうち 5 本できていない(100%)。B: 10 本のうち 2 本(20%)。C: 10 本のうち 0 本。
        let mut rows = vec![];
        for i in 0..5 {
            rows.push(row(&format!("a{i}"), "A", "judged", Some("not_done")));
        }
        for i in 0..10 {
            rows.push(row(
                &format!("b{i}"),
                "B",
                "judged",
                Some(if i < 2 { "not_done" } else { "done" }),
            ));
            rows.push(row(&format!("c{i}"), "C", "judged", Some("done")));
        }
        let members = vec![m("A", "あ"), m("B", "い"), m("C", "う")];
        let dials: HashMap<String, u32> =
            [("A".into(), 10), ("B".into(), 10), ("C".into(), 10)].into();
        let (people, total) = build(
            &members,
            Some(&dials),
            None,
            &[],
            &rows,
            &["early_date".to_string()],
            &ALERT_RULES,
        );
        // チーム全体: 7 / 25 = 28%
        assert_eq!(total.items[0].not_done, 7);
        assert_eq!(total.items[0].applicable, 25);
        assert!((total.items[0].rate_pct.unwrap() - 28.0).abs() < 1e-9);
        let by = |n: &str| people.iter().find(|p| p.name == n).unwrap();
        assert!(by("あ").items[0].flagged, "100% - 28% = 72 >= 20");
        assert!(!by("い").items[0].flagged, "20% は全体より少ない");
        assert!(!by("う").items[0].flagged);
        assert!(!total.items[0].flagged);
    }

    #[test]
    fn item_not_flagged_below_min_applicable_or_gap() {
        // 該当 4 本は 100% でも印なし。差が 19.9 ポイントでも印なし
        let mut rows = vec![];
        for i in 0..4 {
            rows.push(row(&format!("a{i}"), "A", "judged", Some("not_done")));
        }
        for i in 0..100 {
            rows.push(row(
                &format!("b{i}"),
                "B",
                "judged",
                Some(if i < 40 { "not_done" } else { "done" }),
            ));
        }
        let (people, _) = build(
            &[m("A", "あ"), m("B", "い")],
            Some(&HashMap::new()),
            None,
            &[],
            &rows,
            &["early_date".to_string()],
            &ALERT_RULES,
        );
        assert!(!people.iter().find(|p| p.name == "あ").unwrap().items[0].flagged);
        // B: 40%、全体 44/104=42.3%。B は全体より少ない
        assert!(!people.iter().find(|p| p.name == "い").unwrap().items[0].flagged);
        let exactly = TalkAlertRules {
            item_min_applicable: 1,
            item_excess_pct: 20.0,
            ..ALERT_RULES
        };
        // 全体 50%(2/4)、A 100% → 差 50 で印、B(0%) は印なし
        let rows2 = vec![
            row("1", "A", "judged", Some("not_done")),
            row("2", "A", "judged", Some("not_done")),
            row("3", "B", "judged", Some("done")),
            row("4", "B", "judged", Some("done")),
        ];
        let (p2, _) = build(
            &[m("A", "あ"), m("B", "い")],
            Some(&HashMap::new()),
            None,
            &[],
            &rows2,
            &["early_date".to_string()],
            &exactly,
        );
        assert!(p2[0].items[0].flagged ^ p2[1].items[0].flagged);
    }

    #[test]
    fn appointment_rate_flag_needs_100_dials_and_under_half_of_team() {
        // A: 200 本 2 件(1%)、B: 200 本 10 件(5%)、C: 50 本 0 件 (本数不足)。全体 12/450=2.67%、半分 1.33%
        let members = vec![m("A", "あ"), m("B", "い"), m("C", "う")];
        let dials: HashMap<String, u32> =
            [("A".into(), 200), ("B".into(), 200), ("C".into(), 50)].into();
        let mut apo: Vec<Vec<String>> = vec![vec!["A".into()]; 2];
        apo.extend(vec![vec!["B".to_string()]; 10]);
        let (people, total) = build(&members, Some(&dials), None, &apo, &[], &[], &ALERT_RULES);
        assert_eq!(total.dials, Some(450));
        assert_eq!(total.appointments, 12);
        let by = |n: &str| people.iter().find(|p| p.name == n).unwrap();
        assert_eq!(by("あ").appointment_rate_pct, Some(1.0));
        assert!(by("あ").appointment_rate_flagged);
        assert!(!by("い").appointment_rate_flagged);
        assert!(!by("う").appointment_rate_flagged, "架電 100 本未満");
        assert!(!total.appointment_rate_flagged);
    }

    #[test]
    fn team_total_counts_a_deal_once_even_if_two_people_get_credit() {
        // 取引 3 件: B1 だけ / B1 と S1 の両方 / S1 だけ。人ごとは B1=2, S1=2、チーム全体は 3 件
        let deals: Vec<Vec<String>> = vec![
            vec!["B1".into()],
            vec!["B1".into(), "S1".into()],
            vec!["S1".into()],
        ];
        let dials: HashMap<String, u32> = [("B1".into(), 100), ("S1".into(), 100)].into();
        let (people, total) = build(
            &[m("B1", "あ"), m("S1", "い")],
            Some(&dials),
            None,
            &deals,
            &[],
            &[],
            &ALERT_RULES,
        );
        assert_eq!(
            people.iter().map(|p| p.appointments).collect::<Vec<_>>(),
            vec![2, 2]
        );
        assert_eq!(total.appointments, 3);
        assert_eq!(total.appointment_rate_pct, Some(1.5));
        // 絞り込みで S1 だけにすると、S1 に付いた 2 件
        let (_, t2) = build(
            &[m("S1", "い")],
            Some(&dials),
            None,
            &deals,
            &[],
            &[],
            &ALERT_RULES,
        );
        assert_eq!(t2.appointments, 2);
    }

    #[test]
    fn without_the_sheet_there_are_no_dials_and_no_rate() {
        let deals: Vec<Vec<String>> = vec![vec!["A".into()]];
        let (people, total) = build(&[m("A", "あ")], None, None, &deals, &[], &[], &ALERT_RULES);
        assert_eq!(people.len(), 1, "アポのある人は架電数が無くても出る");
        assert_eq!(people[0].dials, None);
        assert_eq!(people[0].appointments, 1);
        assert_eq!(people[0].appointment_rate_pct, None);
        assert!(!people[0].appointment_rate_flagged);
        assert_eq!((total.dials, total.appointment_rate_pct), (None, None));
    }

    #[test]
    fn counts_review_states_and_skips_idle_people() {
        let rows = vec![
            row("1", "A", "judged", None),
            row("2", "A", "pending_transcript", None),
            row("3", "A", "unavailable", None),
            row("4", "A", "failed", None),
            row("5", "Z", "judged", None),
            row("6", "A", "queued", None),
        ];
        let (people, total) = build(
            &[m("A", "あ"), m("B", "い")],
            Some(&HashMap::new()),
            None,
            &[],
            &rows,
            &[],
            &ALERT_RULES,
        );
        assert_eq!(people.len(), 1, "B は何も無いので出ない");
        let a = &people[0];
        // 枠待ちの 1 本は「待ち」に数えず、対象の本数にだけ入る
        assert_eq!((a.reviewed, a.waiting, a.unavailable), (1, 1, 2));
        assert_eq!(a.eligible, 5);
        assert_eq!(
            (total.reviewed, total.waiting, total.unavailable),
            (1, 1, 2)
        );
        assert_eq!(total.eligible, 5);
        assert_eq!(a.appointment_rate_pct, None);
    }

    #[test]
    fn outbound_is_counted_per_person_and_summed_like_dials() {
        let members = vec![m("A", "あ"), m("B", "い")];
        let dials: HashMap<String, u32> = [("A".into(), 40), ("B".into(), 10)].into();
        let out: HashMap<String, u32> = [("A".into(), 90)].into();
        let (people, total) = build(
            &members,
            Some(&dials),
            Some(&out),
            &[],
            &[],
            &[],
            &ALERT_RULES,
        );
        let of = |id: &str| {
            people
                .iter()
                .find(|p| p.owner_id.as_deref() == Some(id))
                .unwrap()
                .outbound
        };
        assert_eq!(of("A"), Some(90));
        assert_eq!(of("B"), Some(0));
        assert_eq!(total.outbound, Some(90));
        let (people, total) = build(&members, Some(&dials), None, &[], &[], &[], &ALERT_RULES);
        assert!(people.iter().all(|p| p.outbound.is_none()));
        assert_eq!(total.outbound, None);
    }
}
