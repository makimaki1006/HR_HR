//! Optional additive snapshot fields are validated without changing old snapshots.
use super::{
    applicant_reasons::{
        Reasons, CATEGORY_PROPERTIES, LEGACY_PROPERTIES, MAX_ITEMS, MAX_TEXT_CHARS,
        MAX_VALUE_CHARS, PROPERTIES, TEXT_PROPERTIES,
    },
    keys_only,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn opaque_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn date(value: &Value) -> Option<chrono::NaiveDate> {
    let raw = value.as_str()?;
    let date = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
    (date.to_string() == raw).then_some(date)
}
pub(super) fn validate(result: &Value, job: &Value) -> bool {
    let total = result["summary"]["total"].as_u64().unwrap_or(u64::MAX);
    if let Some(value) = result.get("applicant_reasons") {
        let Ok(r) = serde_json::from_value::<Reasons>(value.clone()) else {
            return false;
        };
        if !r.available
            || r.source != "hubspot"
            || r.basis != "recorded_applicant_reason"
            || r.source_property.is_some()
            || chrono::DateTime::parse_from_rfc3339(&r.fetched_at).is_err()
            || r.total_applicants as u64 != total
            || r.items.len() > MAX_ITEMS
        {
            return false;
        }
        // A snapshot written before 2026-10-08 has the three old sources, no applicant keys and
        // no selections. A newer one has every source, applicant keys and selections.
        let legacy = r.source_counts.len() == LEGACY_PROPERTIES.len();
        let sources: &[&str] = if legacy {
            &LEGACY_PROPERTIES
        } else {
            &PROPERTIES
        };
        let text_sources: &[&str] = if legacy {
            &LEGACY_PROPERTIES
        } else {
            &TEXT_PROPERTIES
        };
        if r.source_counts.len() != sources.len()
            || r.total_applicants.checked_mul(sources.len()) != Some(r.total_source_values)
            || legacy != r.selections.is_none()
            || r.items.iter().any(|item| match &item.applicant {
                None => !legacy,
                Some(key) => legacy || !opaque_key(key),
            })
        {
            return false;
        }
        let mut missing = 0usize;
        let mut blank = 0usize;
        let mut text_nonblank = 0usize;
        for property in sources {
            let property = *property;
            let Some(c) = r.source_counts.get(property) else {
                return false;
            };
            if c.missing
                .checked_add(c.blank)
                .and_then(|n| n.checked_add(c.nonblank))
                != Some(r.total_applicants)
            {
                return false;
            }
            missing += c.missing;
            blank += c.blank;
            if CATEGORY_PROPERTIES.contains(&property) {
                let selected: BTreeSet<_> = r
                    .selections
                    .iter()
                    .flatten()
                    .filter(|s| s.source_property == property)
                    .map(|s| s.applicant.as_str())
                    .collect();
                if selected.len() != c.nonblank {
                    return false;
                }
                continue;
            }
            text_nonblank += c.nonblank;
            let shown = r
                .items
                .iter()
                .filter(|item| item.source_property == property)
                .count();
            if shown > c.nonblank || (!r.truncated && shown != c.nonblank) {
                return false;
            }
        }
        if missing != r.missing
            || blank != r.blank
            || r.items.len() > text_nonblank
            || (!r.truncated && r.items.len() != text_nonblank)
        {
            return false;
        }
        if let Some(keys) = &r.multi_listing_applicants {
            let mut unique = BTreeSet::new();
            if legacy
                || keys.len() > r.total_applicants
                || keys
                    .iter()
                    .any(|key| !opaque_key(key) || !unique.insert(key))
            {
                return false;
            }
        }
        let mut chosen = BTreeSet::new();
        for selection in r.selections.iter().flatten() {
            if !CATEGORY_PROPERTIES.contains(&selection.source_property.as_str())
                || !opaque_key(&selection.applicant)
                || selection.value.trim().is_empty()
                || selection.value.chars().count() > MAX_VALUE_CHARS
                || selection
                    .label
                    .as_ref()
                    .is_some_and(|l| l.trim().is_empty() || l.chars().count() > MAX_VALUE_CHARS)
                || !chosen.insert((
                    selection.applicant.as_str(),
                    selection.source_property.as_str(),
                    selection.value.as_str(),
                ))
                || selection
                    .application_date
                    .as_ref()
                    .is_some_and(|d| date(&Value::String(d.clone())).is_none())
            {
                return false;
            }
        }
        let mut ids = BTreeSet::new();
        for item in r.items {
            if item.source != "hubspot"
                || !text_sources.contains(&item.source_property.as_str())
                || item.id.len() != 64
                || !item
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || !ids.insert(item.id)
                || item.text.trim().is_empty()
                || item.text.chars().count() > MAX_TEXT_CHARS
                || item.collected_at.is_some()
                || item.version_id.is_some()
                || item
                    .application_date
                    .as_ref()
                    .is_some_and(|d| date(&Value::String(d.clone())).is_none())
            {
                return false;
            }
        }
    }
    if let Some(joint) = result["summary"].get("joint_demographics") {
        if !keys_only(joint, &["total", "cells"]) || joint["total"].as_u64() != Some(total) {
            return false;
        }
        let Some(cells) = joint["cells"].as_array() else {
            return false;
        };
        if cells.len() > 10000 {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut sum = 0u64;
        let mut marginals: BTreeMap<&str, BTreeMap<String, u64>> = BTreeMap::new();
        for cell in cells {
            if !keys_only(
                cell,
                &["gender", "age", "prefecture", "municipality", "count"],
            ) {
                return false;
            }
            let Some(count) = cell["count"].as_u64().filter(|n| *n > 0) else {
                return false;
            };
            let mut labels = Vec::new();
            for dimension in ["gender", "age", "prefecture", "municipality"] {
                let Some(label) = cell[dimension]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 4096)
                else {
                    return false;
                };
                labels.push(label);
                let entry = marginals
                    .entry(dimension)
                    .or_default()
                    .entry(label.to_owned())
                    .or_default();
                let Some(next) = entry.checked_add(count) else {
                    return false;
                };
                *entry = next;
            }
            if !seen.insert(labels) {
                return false;
            }
            let Some(next) = sum.checked_add(count) else {
                return false;
            };
            sum = next;
        }
        if sum != total {
            return false;
        }
        for (dimension, marginal) in marginals {
            if serde_json::to_value(marginal).ok().as_ref()
                != Some(&result["summary"]["dimensions"][dimension])
            {
                return false;
            }
        }
    }
    if let Some(performance) = result.get("hrh_performance") {
        if !keys_only(
            performance,
            &["schema_version", "source", "job_id", "captured_at", "rows"],
        ) || performance["schema_version"].as_u64() != Some(1)
            || performance["source"] != "hrhacker"
            || performance["job_id"]
                .as_str()
                .is_none_or(|id| id.len() != 8 || !id.bytes().all(|c| c.is_ascii_digit()))
            || performance["job_id"] != job["mediaJobId"]
            || job["media"] != "HRハッカー"
            || performance["captured_at"]
                .as_str()
                .is_none_or(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
        {
            return false;
        }
        let Some(rows) = performance["rows"].as_array().filter(|r| r.len() <= 1000) else {
            return false;
        };
        let mut periods = Vec::new();
        for row in rows {
            if !keys_only(
                row,
                &[
                    "period_start",
                    "period_end",
                    "impressions",
                    "clicks",
                    "cost_yen",
                    "applications",
                ],
            ) {
                return false;
            }
            let (Some(start), Some(end)) = (date(&row["period_start"]), date(&row["period_end"]))
            else {
                return false;
            };
            if start > end {
                return false;
            }
            periods.push((start, end));
            for field in ["impressions", "clicks", "applications"] {
                if row.get(field).is_none()
                    || (!row[field].is_null() && row[field].as_u64().is_none())
                {
                    return false;
                }
            }
            if row.get("cost_yen").is_none()
                || (!row["cost_yen"].is_null()
                    && row["cost_yen"]
                        .as_f64()
                        .is_none_or(|n| !n.is_finite() || n < 0.0))
            {
                return false;
            }
            if let (Some(clicks), Some(impressions)) =
                (row["clicks"].as_u64(), row["impressions"].as_u64())
            {
                if clicks > impressions {
                    return false;
                }
            }
        }
        periods.sort();
        if periods.windows(2).any(|p| p[0].1 >= p[1].0) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (Value, Value) {
        let rows = [super::super::Record {
            id: "50".into(),
            properties: BTreeMap::from([("oubodouki".into(), Some("Recorded reason".into()))]),
        }];
        (
            json!({"summary":super::super::summarize(&rows),"applicant_reasons":super::super::applicant_reasons::extract("30",&rows,"2026-10-05T00:00:00Z".into())}),
            json!({"media":"HRハッカー","mediaJobId":"12345678"}),
        )
    }
    #[test]
    fn reasons_and_joint_validate_and_preserve_old_optional_contract() {
        let (mut result, job) = fixture();
        assert!(validate(&result, &job));
        result.as_object_mut().unwrap().remove("applicant_reasons");
        result["summary"]
            .as_object_mut()
            .unwrap()
            .remove("joint_demographics");
        assert!(validate(&result, &job));
    }
    #[test]
    fn snapshot_written_before_the_new_sources_still_validates_and_mixed_shapes_do_not() {
        let (result, job) = fixture();
        let mut legacy = result.clone();
        let reasons = &mut legacy["applicant_reasons"];
        reasons.as_object_mut().unwrap().remove("selections");
        let counts = reasons["source_counts"].as_object_mut().unwrap();
        for property in [
            "genshokumaeshokukaranotenshokuriyuu",
            "ouboriyuukategori_hiaringu",
            "ouboriyuukategori_baitaikisai",
        ] {
            counts.remove(property);
        }
        reasons["total_source_values"] = json!(3);
        reasons["missing"] = json!(2);
        reasons["items"][0]
            .as_object_mut()
            .unwrap()
            .remove("applicant");
        assert!(validate(&legacy, &job));
        // An old shape with an applicant key, or a new shape without one, is rejected.
        let mut bad = legacy.clone();
        bad["applicant_reasons"]["items"][0]["applicant"] = json!("a".repeat(64));
        assert!(!validate(&bad, &job));
        let mut bad = result.clone();
        bad["applicant_reasons"]["items"][0]
            .as_object_mut()
            .unwrap()
            .remove("applicant");
        assert!(!validate(&bad, &job));
        let mut bad = result;
        bad["applicant_reasons"]
            .as_object_mut()
            .unwrap()
            .remove("selections");
        assert!(!validate(&bad, &job));
    }
    #[test]
    fn snapshots_the_server_writes_validate_including_separator_only_selects_and_old_dumps() {
        let rows = [
            super::super::Record {
                id: "50".into(),
                properties: BTreeMap::from([
                    ("ouboriyuukategori_hiaringu".into(), Some(";".into())),
                    ("oubodouki".into(), Some("家から近い".into())),
                ]),
            },
            super::super::Record {
                id: "51".into(),
                properties: BTreeMap::from([(
                    "ouboriyuukategori_baitaikisai".into(),
                    Some("給与; ;".into()),
                )]),
            },
        ];
        let summary = super::super::summarize(&rows);
        let reasons =
            super::super::applicant_reasons::extract("30", &rows, "2026-10-05T00:00:00Z".into());
        assert!(validate(
            &json!({"summary":summary,"applicant_reasons":reasons}),
            &json!({})
        ));
        let legacy = super::super::applicant_reasons::extract_legacy(
            "30",
            &rows,
            "2026-10-05T00:00:00Z".into(),
        );
        assert!(validate(
            &json!({"summary":summary,"applicant_reasons":legacy}),
            &json!({})
        ));
    }
    #[test]
    fn multi_listing_keys_are_opaque_unique_and_only_in_the_current_shape() {
        let (mut result, job) = fixture();
        let key = super::super::applicant_reasons::applicant_key("30", "50");
        result["applicant_reasons"]["multi_listing_applicants"] = json!([key]);
        assert!(validate(&result, &job));
        result["applicant_reasons"]["multi_listing_applicants"] = json!([]);
        assert!(validate(&result, &job));
        for bad_keys in [json!(["50"]), json!([&key, &key])] {
            let mut bad = result.clone();
            bad["applicant_reasons"]["multi_listing_applicants"] = bad_keys;
            assert!(!validate(&bad, &job));
        }
        // An old shape (no applicant keys) cannot name the applications.
        let rows = [super::super::Record {
            id: "50".into(),
            properties: BTreeMap::from([("oubodouki".into(), Some("Recorded reason".into()))]),
        }];
        let mut legacy = super::super::applicant_reasons::extract_legacy(
            "30",
            &rows,
            "2026-10-05T00:00:00Z".into(),
        );
        legacy.mark_multi_listing("30", &rows, &BTreeSet::from(["50".to_owned()]));
        assert!(legacy.multi_listing_applicants.is_none());
        let mut bad = json!({"summary":super::super::summarize(&rows),"applicant_reasons":legacy});
        bad["applicant_reasons"]["multi_listing_applicants"] = json!([key]);
        assert!(!validate(&bad, &job));
    }
    #[test]
    fn selections_must_match_the_category_counts() {
        let rows = [super::super::Record {
            id: "50".into(),
            properties: BTreeMap::from([(
                "ouboriyuukategori_hiaringu".into(),
                Some("給与".into()),
            )]),
        }];
        let result = json!({"summary":super::super::summarize(&rows),"applicant_reasons":super::super::applicant_reasons::extract("30",&rows,"2026-10-05T00:00:00Z".into())});
        let job = json!({});
        assert!(validate(&result, &job));
        let mut bad = result.clone();
        bad["applicant_reasons"]["selections"] = json!([]);
        assert!(!validate(&bad, &job));
        let mut bad = result.clone();
        bad["applicant_reasons"]["selections"][0]["source_property"] = json!("oubodouki");
        assert!(!validate(&bad, &job));
        let mut bad = result;
        bad["applicant_reasons"]["selections"][0]["applicant"] = json!("50");
        assert!(!validate(&bad, &job));
    }
    #[test]
    fn reason_person_fields_inferred_versions_and_bad_counts_rejected() {
        let (result, job) = fixture();
        for (pointer, value) in [
            ("/applicant_reasons/items/0/version_id", json!("inferred")),
            ("/applicant_reasons/missing", json!(0)),
            ("/applicant_reasons/items/0/id", json!("50")),
        ] {
            let mut bad = result.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(!validate(&bad, &job));
        }
        let mut bad = result;
        bad["applicant_reasons"]["items"][0]["email"] = json!("fictional@example.invalid");
        assert!(!validate(&bad, &job));
    }
    #[test]
    fn joint_cells_are_actual_deduplicated_observations_and_conserve_marginals() {
        let (result, job) = fixture();
        for (pointer, value) in [
            ("/summary/joint_demographics/cells/0/count", json!(2)),
            ("/summary/joint_demographics/cells/0/age", json!("30代")),
        ] {
            let mut bad = result.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(!validate(&bad, &job));
        }
    }
    #[test]
    fn joint_cells_do_not_form_a_marginal_cross_product() {
        let row = |id: &str, gender: &str, age: &str| super::super::Record {
            id: id.into(),
            properties: BTreeMap::from([
                ("seibetsu".into(), Some(gender.into())),
                ("nenrei".into(), Some(age.into())),
            ]),
        };
        let a = row("50", "A", "31");
        let b = row("51", "B", "45");
        let summary = super::super::summarize(&[a.clone(), a, b]);
        assert_eq!(summary["total"], 2);
        assert_eq!(summary["duplicate_ids"], 1);
        let cells = summary["joint_demographics"]["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0]["gender"], "A");
        assert_eq!(cells[0]["age"], "30代");
        assert_eq!(cells[1]["gender"], "B");
        assert_eq!(cells[1]["age"], "40代");
        assert!(validate(&json!({"summary":summary}), &json!({})));
    }
    #[test]
    fn performance_rejects_wrong_job_overlap_dates_coercion_and_extra_fields() {
        let (mut result, job) = fixture();
        result["hrh_performance"] = json!({"schema_version":1,"source":"hrhacker","job_id":"12345678","captured_at":"2026-10-05T00:00:00Z","rows":[{"period_start":"2026-10-01","period_end":"2026-10-03","impressions":10,"clicks":2,"cost_yen":5.5,"applications":null}]});
        assert!(validate(&result, &job));
        for (pointer, value) in [
            ("/hrh_performance/job_id", json!("87654321")),
            ("/hrh_performance/rows/0/period_start", json!("2026-02-30")),
            ("/hrh_performance/rows/0/clicks", json!(11)),
            ("/hrh_performance/rows/0/impressions", json!("10")),
            ("/hrh_performance/rows/0/cost_yen", json!(-1)),
        ] {
            let mut bad = result.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(!validate(&bad, &job));
        }
        let mut bad = result.clone();
        let row = bad["hrh_performance"]["rows"][0].clone();
        bad["hrh_performance"]["rows"]
            .as_array_mut()
            .unwrap()
            .push(row);
        assert!(!validate(&bad, &job));
        result["hrh_performance"]["secret"] = json!("not allowed");
        assert!(!validate(&result, &job));
    }
}
