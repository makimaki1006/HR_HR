//! 取引内の応募を、選択式の記録が始まった応募日から数える。
use super::{apply_reasons::SELECT_PROPERTIES, todo_signals::Facts, *};
use chrono::NaiveDate;

#[derive(Serialize, Deserialize, ts_rs::TS)]
pub(super) struct DealApplicationReasonChoice {
    label: String,
    unknown: bool,
    counts: Vec<u32>,
}
#[derive(Serialize, Deserialize, ts_rs::TS)]
pub(super) struct DealApplicationReasonSummary {
    total: u32,
    answered: Vec<u32>,
    top: Vec<DealApplicationReasonChoice>,
    motives: Vec<DealApplicationReasonChoice>,
    priorities: Vec<DealApplicationReasonChoice>,
}

pub(super) fn summarize(
    facts: Option<&Facts>,
    definitions: Option<&BTreeMap<String, Vec<Value>>>,
    today: NaiveDate,
) -> Option<DealApplicationReasonSummary> {
    let facts = facts?;
    let selections = facts.selections.as_ref()?;
    // 定義を読めない場合も、順序や0件を推測せず次の読み直しを待つ。
    let definitions = definitions?;
    if SELECT_PROPERTIES
        .iter()
        .any(|p| !definitions.contains_key(*p))
    {
        return None;
    }
    let since = NaiveDate::from_ymd_opt(2026, 9, 14).expect("selection start");
    let mut counts = vec![BTreeMap::<String, u32>::new(); 4];
    let mut answered = vec![0; 4];
    let mut total = 0;
    for (id, day) in &facts.application_days {
        if !day.is_some_and(|d| d >= since && d <= today) {
            continue;
        }
        total += 1;
        for (index, property) in SELECT_PROPERTIES.iter().enumerate() {
            let values: BTreeSet<_> = selections
                .get(id)
                .and_then(|s| s.get(*property))
                .into_iter()
                .flatten()
                .filter(|value| {
                    let label = definitions
                        .get(*property)
                        .into_iter()
                        .flatten()
                        .find(|o| o["value"].as_str() == Some(value.as_str()))
                        .and_then(|o| o["label"].as_str());
                    !value.trim().is_empty() && !apply_reasons::is_unset(value, label)
                })
                .collect();
            if !values.is_empty() {
                answered[index] += 1;
            }
            for value in values {
                *counts[index].entry(value.clone()).or_default() += 1;
            }
        }
    }
    let table = |indices: &[usize]| {
        let options = definitions.get(SELECT_PROPERTIES[indices[0]]);
        let mut rows: Vec<_> = options
            .into_iter()
            .flatten()
            .filter_map(|option| {
                let label = option["label"].as_str()?;
                Some(DealApplicationReasonChoice {
                    label: label.to_owned(),
                    unknown: false,
                    counts: indices
                        .iter()
                        .map(|i| {
                            let value = definitions
                                .get(SELECT_PROPERTIES[*i])
                                .into_iter()
                                .flatten()
                                .find(|o| o["label"].as_str() == Some(label))
                                .and_then(|o| o["value"].as_str());
                            value.and_then(|v| counts[*i].get(v)).copied().unwrap_or(0)
                        })
                        .collect(),
                })
            })
            .collect();
        let unknown: BTreeSet<_> = indices
            .iter()
            .flat_map(|i| {
                counts[*i].keys().filter(|v| {
                    !definitions
                        .get(SELECT_PROPERTIES[*i])
                        .into_iter()
                        .flatten()
                        .any(|o| o["value"].as_str() == Some(v.as_str()))
                })
            })
            .collect();
        for value in unknown {
            rows.push(DealApplicationReasonChoice {
                label: applicant_reasons::mask_personal_details(value),
                unknown: true,
                counts: indices
                    .iter()
                    .map(|i| {
                        let defined = definitions
                            .get(SELECT_PROPERTIES[*i])
                            .into_iter()
                            .flatten()
                            .any(|o| o["value"].as_str() == Some(value.as_str()));
                        if defined {
                            0
                        } else {
                            counts[*i].get(value).copied().unwrap_or(0)
                        }
                    })
                    .collect(),
            });
        }
        rows
    };
    let motives = table(&[0]);
    let priorities = table(&[1, 2, 3]);
    let mut ranked: Vec<_> = motives.iter().filter(|r| r.counts[0] > 0).collect();
    // 安定ソートで、同数は定義の displayOrder を保つ。
    ranked.sort_by_key(|r| std::cmp::Reverse(r.counts[0]));
    let top = ranked
        .into_iter()
        .take(2)
        .map(|r| DealApplicationReasonChoice {
            label: r.label.clone(),
            unknown: r.unknown,
            counts: r.counts.clone(),
        })
        .collect();
    Some(DealApplicationReasonSummary {
        total,
        answered,
        top,
        motives,
        priorities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_reasons_skip_early_unknown_future_and_keep_definition_order_on_ties() {
        let day = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let definitions =
            apply_reasons::parse_options(&json!({"results": SELECT_PROPERTIES.map(|p| json!({
                "name":p,"options":[
                    {"value":"給与","label":"給与","displayOrder":2},
                    {"value":"休日","label":"休日","displayOrder":1},
                    {"value":"通勤","label":"通勤","displayOrder":3}
                ]
            }))}))
            .unwrap()
            .definitions;
        let mut facts = Facts {
            application_days: [
                ("a", Some("2026-09-14")),
                ("b", Some("2026-09-15")),
                ("old", Some("2026-09-13")),
                ("unknown", None),
                ("future", Some("2027-01-01")),
                ("empty", Some("2026-09-16")),
            ]
            .into_iter()
            .map(|(id, d)| (id.into(), d.map(day)))
            .collect(),
            selections: Some(
                ["a", "b", "old", "unknown", "future"]
                    .into_iter()
                    .map(|id| {
                        (
                            id.into(),
                            BTreeMap::from([
                                (
                                    SELECT_PROPERTIES[0].into(),
                                    vec![
                                        "給与".into(),
                                        "休日".into(),
                                        "給与".into(),
                                        "旧選択肢".into(),
                                        "unset".into(),
                                    ],
                                ),
                                (SELECT_PROPERTIES[1].into(), vec!["休日".into()]),
                                (SELECT_PROPERTIES[2].into(), vec!["給与".into()]),
                                (SELECT_PROPERTIES[3].into(), vec!["旧選択肢".into()]),
                            ]),
                        )
                    })
                    .collect(),
            ),
            ..Default::default()
        };
        let summary = serde_json::to_value(summarize(
            Some(&facts),
            Some(&definitions),
            day("2026-11-09"),
        ))
        .unwrap();
        assert_eq!(summary["total"], 3);
        assert_eq!(summary["answered"], json!([2, 2, 2, 2]));
        assert_eq!(
            summary["top"],
            json!([
                {"label":"休日","unknown":false,"counts":[2]},
                {"label":"給与","unknown":false,"counts":[2]}
            ])
        );
        assert_eq!(summary["motives"][2]["counts"], json!([0]));
        assert_eq!(
            summary["motives"][3],
            json!({"label":"旧選択肢","unknown":true,"counts":[2]})
        );
        assert_eq!(summary["priorities"][0]["counts"], json!([2, 0, 0]));
        assert_eq!(summary["priorities"][1]["counts"], json!([0, 2, 0]));
        assert_eq!(summary["priorities"][3]["counts"], json!([0, 0, 2]));
        facts.selections = Some(BTreeMap::new());
        let empty = summarize(Some(&facts), Some(&definitions), day("2026-11-09")).unwrap();
        assert_eq!(empty.answered, [0, 0, 0, 0]);
        assert!(empty.top.is_empty());
    }
    #[test]
    fn old_snapshot_keeps_existing_numbers_and_reasons_pending() {
        let facts: Facts = serde_json::from_value(json!({"live":2,"unknown_dates":0,
            "application_days":{"a":"2026-09-14"},"days":["2026-09-14"],"listings":1}))
        .unwrap();
        assert_eq!(facts.live, 2);
        assert!(facts.selections.is_none());
        assert!(summarize(
            Some(&facts),
            Some(&BTreeMap::new()),
            NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
        )
        .is_none());
    }
}
