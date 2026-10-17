//! 人の予定入力とは独立した、事実から消長する要対応。
use super::owner_todo::{date, WorkDeal, ENDED_STAGES};
use super::*;
use chrono::NaiveDate;

// ユーザー決定 2026-10-16。判定に使う閾値の正本。
pub(super) const EXPIRATION_DAYS: i64 = 60;
const CONTACT_DAYS: i64 = 30;
const APPLICATION_DAYS: i64 = 14;
const FIRST_CONTRACT_TARGETS: [(i64, usize); 2] = [(30, 5), (60, 10)];
const FIRST_CONTRACT_VALUE: &str = "新規契約";

#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub(super) struct Facts {
    pub(super) live: usize,
    // 応募 ID は取引ごとに重複排除してから日付だけを保持する。
    pub(super) days: Vec<NaiveDate>,
}

pub(super) fn reasons(deal: &WorkDeal, facts: Option<&Facts>, today: NaiveDate) -> Vec<Value> {
    if !listings::is_delivery(deal.record.value("pipeline"))
        || ENDED_STAGES.contains(&deal.record.value("dealstage").unwrap_or(""))
    {
        return vec![];
    }
    let mut reasons = vec![];
    let mut add = |kind: &str, text: String| reasons.push(json!({"kind":kind,"text":text}));
    if let Some(days) = deal
        .assignment
        .contract_expiration_date
        .as_deref()
        .and_then(date)
        .map(|d| (d - today).num_days())
        .filter(|d| (0..=EXPIRATION_DAYS).contains(d))
    {
        add("expiration", format!("満了まで {days} 日"));
    }
    let elapsed = deal
        .record
        .value("contract_start_date")
        .and_then(date)
        .map(|d| (today - d).num_days());
    match deal.record.value("notes_last_contacted").and_then(date) {
        Some(last) if (today - last).num_days() >= CONTACT_DAYS => add(
            "contact",
            format!("最後の接触から {} 日", (today - last).num_days()),
        ),
        None if elapsed.is_some_and(|n| n >= CONTACT_DAYS) => {
            add("contact", "接触の記録が無い".into())
        }
        _ => {}
    }
    if let (Some(days), Some(facts)) = (elapsed, facts) {
        // 直近14暦日 = 今日とその前の13日。未来の応募は含めない。
        let recent = facts
            .days
            .iter()
            .filter(|d| (0..APPLICATION_DAYS).contains(&(today - **d).num_days()))
            .count();
        if days >= APPLICATION_DAYS && facts.live > 0 && recent == 0 {
            add(
                "stalled",
                format!(
                    "直近{APPLICATION_DAYS}日の応募 0 件（掲載中 {} 件）",
                    facts.live
                ),
            );
        }
        let total = facts
            .days
            .iter()
            .filter(|d| (0..=days).contains(&(today - **d).num_days()))
            .count();
        if deal.record.value("contract_type") == Some(FIRST_CONTRACT_VALUE)
            && FIRST_CONTRACT_TARGETS
                .iter()
                .any(|(age, target)| days >= *age && total < *target)
        {
            add(
                "first_contract",
                format!("初回契約・開始から {days} 日で応募 {total} 件"),
            );
        }
    }
    reasons
}

pub(super) async fn fetch(
    service: &JobReadService,
    deals: &BTreeMap<String, WorkDeal>,
) -> Result<BTreeMap<String, Facts>, ReadError> {
    // 契約開始のない案件は応募条件を満たさない。翌日の境界分も用意する。
    let ids: Vec<_> = deals
        .iter()
        .filter(|(_, d)| {
            !ENDED_STAGES.contains(&d.record.value("dealstage").unwrap_or(""))
                && d.record
                    .value("contract_start_date")
                    .and_then(date)
                    .is_some()
        })
        .map(|(id, _)| id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let relations = service.association_map("deals", &ids, "0-420").await?;
    let listing_ids: Vec<_> = relations
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let live = listings::todo_publications(service, &listing_ids).await?;
    let applications = service
        .association_map("0-420", &listing_ids, "0-421")
        .await?;
    let ids: Vec<_> = applications
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let dates: BTreeMap<_, _> = service
        .batch("0-421", &ids, &["yingmuri"])
        .await?
        .into_iter()
        .map(|r| {
            (
                r.id.clone(),
                r.value("yingmuri")
                    .and_then(super::deal_summary::jst_day)
                    .as_deref()
                    .and_then(date),
            )
        })
        .collect();
    Ok(assemble(relations, &applications, &dates, &live))
}

fn assemble(
    relations: BTreeMap<String, Vec<String>>,
    applications: &BTreeMap<String, Vec<String>>,
    dates: &BTreeMap<String, Option<NaiveDate>>,
    live: &BTreeSet<String>,
) -> BTreeMap<String, Facts> {
    relations
        .into_iter()
        .map(|(deal, listings)| {
            let apps: BTreeSet<_> = listings
                .iter()
                .flat_map(|id| applications.get(id).into_iter().flatten())
                .collect();
            let facts = Facts {
                live: listings.iter().filter(|id| live.contains(*id)).count(),
                days: apps
                    .into_iter()
                    .filter_map(|id| dates.get(id).copied().flatten())
                    .collect(),
            };
            (deal, facts)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::owner_todo::Assignment;
    use super::*;
    fn day(s: &str) -> NaiveDate {
        date(s).unwrap()
    }
    fn deal(age: i64, contact: Option<&str>, expiration: i64) -> WorkDeal {
        let today = day("2026-10-16");
        let record = Record {
            id: "1".into(),
            properties: [
                ("pipeline", Some("21596025".into())),
                ("dealstage", Some("open".into())),
                (
                    "contract_start_date",
                    Some((today - chrono::Duration::days(age)).to_string()),
                ),
                (
                    "contract_expiration_date",
                    Some((today + chrono::Duration::days(expiration)).to_string()),
                ),
                ("contract_type", Some("新規契約".into())),
                ("notes_last_contacted", contact.map(str::to_owned)),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect(),
        };
        WorkDeal {
            assignment: Assignment::from_record(&record),
            record,
        }
    }
    fn kinds(d: &WorkDeal, f: &Facts) -> Vec<String> {
        reasons(d, Some(f), day("2026-10-16"))
            .iter()
            .map(|v| v["kind"].as_str().unwrap().into())
            .collect()
    }
    #[test]
    fn one_application_linked_to_two_listings_counts_once_and_unknown_date_is_not_zero_day() {
        let relations = BTreeMap::from([("1".into(), vec!["10".into(), "20".into()])]);
        let applications = BTreeMap::from([
            ("10".into(), vec!["100".into(), "200".into()]),
            ("20".into(), vec!["100".into(), "300".into()]),
        ]);
        let dates = BTreeMap::from([
            ("100".into(), Some(day("2026-10-16"))),
            ("200".into(), None),
            ("300".into(), Some(day("2026-10-01"))),
        ]);
        let facts = assemble(
            relations,
            &applications,
            &dates,
            &BTreeSet::from(["10".into()]),
        );
        assert_eq!(facts["1"].live, 1);
        assert_eq!(facts["1"].days, [day("2026-10-16"), day("2026-10-01")]);
    }
    #[test]
    fn exact_expiration_and_contact_boundaries_in_jst() {
        let f = Facts::default();
        for (n, has) in [(-1, false), (0, true), (60, true), (61, false)] {
            assert_eq!(
                kinds(&deal(0, Some("2026-10-16"), n), &f).contains(&"expiration".into()),
                has
            );
        }
        for (contact, has) in [
            ("2026-09-16T14:59:59Z", true),
            ("2026-09-16T15:00:00Z", false),
            ("2026-10-17", false),
        ] {
            assert_eq!(
                kinds(&deal(0, Some(contact), 61), &f).contains(&"contact".into()),
                has
            );
        }
        for (age, has) in [(29, false), (30, true)] {
            assert_eq!(
                kinds(&deal(age, None, 61), &f).contains(&"contact".into()),
                has
            );
        }
        assert_eq!(
            reasons(&deal(30, None, 61), Some(&f), day("2026-10-16"))[0]["text"],
            "接触の記録が無い"
        );
    }
    #[test]
    fn stalled_fourteen_calendar_days_and_start_boundary() {
        let today = day("2026-10-16");
        let mut f = Facts {
            live: 2,
            days: vec![],
        };
        for (age, has) in [(13, false), (14, true)] {
            assert_eq!(
                kinds(&deal(age, Some("2026-10-16"), 61), &f).contains(&"stalled".into()),
                has
            );
        }
        for (since, has) in [(-1, true), (0, false), (13, false), (14, true)] {
            f.days = vec![today - chrono::Duration::days(since)];
            assert_eq!(
                kinds(&deal(14, Some("2026-10-16"), 61), &f).contains(&"stalled".into()),
                has
            );
        }
        f.live = 0;
        f.days.clear();
        assert!(!kinds(&deal(14, None, 61), &f).contains(&"stalled".into()));
    }
    #[test]
    fn first_contract_targets_and_auto_disappearance() {
        for (age, total, has) in [
            (29, 0, false),
            (30, 4, true),
            (30, 5, false),
            (59, 5, false),
            (60, 9, true),
            (60, 10, false),
            (61, 10, false),
        ] {
            let d = deal(age, Some("2026-10-16"), 61);
            let f = Facts {
                live: 1,
                days: vec![day("2026-10-16"); total],
            };
            assert_eq!(
                kinds(&d, &f).contains(&"first_contract".into()),
                has,
                "age={age} count={total}"
            );
        }
        let mut d = deal(60, None, 0);
        let mut f = Facts {
            live: 1,
            days: vec![],
        };
        assert_eq!(
            kinds(&d, &f),
            ["expiration", "contact", "stalled", "first_contract"]
        );
        d.record
            .properties
            .insert("notes_last_contacted".into(), Some("2026-10-16".into()));
        d.assignment.contract_expiration_date = Some("2027-01-01".into());
        f.days = vec![day("2026-10-16"); 10];
        assert!(kinds(&d, &f).is_empty());
        f.days = vec![day("2026-08-16"); 10]; // 開始前の応募を除く。
        assert!(kinds(&d, &f).contains(&"first_contract".into()));
        d.record
            .properties
            .insert("contract_type".into(), Some("FS - 再契約".into()));
        assert!(!kinds(&d, &f).contains(&"first_contract".into()));
        for stage in ENDED_STAGES {
            d.record
                .properties
                .insert("dealstage".into(), Some(stage.into()));
            assert!(kinds(&d, &f).is_empty());
        }
        d.record
            .properties
            .insert("dealstage".into(), Some("open".into()));
        d.record
            .properties
            .insert("pipeline".into(), Some("other".into()));
        assert!(kinds(&d, &f).is_empty());
    }
}
