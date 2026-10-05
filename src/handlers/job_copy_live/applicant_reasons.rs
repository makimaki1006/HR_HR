//! Bounded recorded reasons; neither causal claims nor copy-version attribution.
use super::Record;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const PROPERTIES: [&str; 3] = ["oubodouki", "ouboriyuu_baitaikisai", "ouboriyuu_hiaringu"];
pub const MAX_ITEMS: usize = 100;
pub const MAX_TEXT_CHARS: usize = 2000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reasons {
    pub available: bool,
    pub source: String,
    pub basis: String,
    pub source_property: Option<String>,
    pub fetched_at: String,
    pub total_applicants: usize,
    pub total_source_values: usize,
    pub source_counts: BTreeMap<String, SourceCounts>,
    pub items: Vec<Reason>,
    pub missing: usize,
    pub blank: usize,
    pub truncated: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCounts {
    pub missing: usize,
    pub blank: usize,
    pub nonblank: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reason {
    pub id: String,
    pub text: String,
    pub source: String,
    pub source_property: String,
    pub application_date: Option<String>,
    pub collected_at: Option<String>,
    pub version_id: Option<String>,
}

fn date(raw: &str) -> Option<String> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .map(|date| date.to_string())
        .filter(|canonical| canonical == raw)
}

pub fn extract(listing: &str, rows: &[Record], fetched_at: String) -> Reasons {
    // Same deterministic duplicate handling as the existing aggregate summary.
    let unique: BTreeMap<_, _> = rows.iter().map(|row| (&row.id, row)).collect();
    let mut reasons = Reasons {
        available: true,
        source: "hubspot".into(),
        basis: "recorded_applicant_reason".into(),
        source_property: None,
        fetched_at,
        total_applicants: unique.len(),
        total_source_values: unique.len() * PROPERTIES.len(),
        source_counts: PROPERTIES
            .iter()
            .map(|key| ((*key).into(), SourceCounts::default()))
            .collect(),
        items: Vec::new(),
        missing: 0,
        blank: 0,
        truncated: false,
    };
    for row in unique.values() {
        for property in PROPERTIES {
            let counts = reasons
                .source_counts
                .get_mut(property)
                .expect("fixed source property");
            let Some(raw) = row.properties.get(property).and_then(Option::as_deref) else {
                counts.missing += 1;
                reasons.missing += 1;
                continue;
            };
            let text = raw.trim();
            if text.is_empty() {
                counts.blank += 1;
                reasons.blank += 1;
                continue;
            }
            counts.nonblank += 1;
            if reasons.items.len() == MAX_ITEMS {
                reasons.truncated = true;
                continue;
            }
            let mut hash = Sha256::new();
            for part in [listing, row.id.as_str(), property] {
                hash.update(part.as_bytes());
                hash.update([0]);
            }
            let bounded: String = text.chars().take(MAX_TEXT_CHARS).collect();
            reasons.truncated |= text.chars().count() > MAX_TEXT_CHARS;
            reasons.items.push(Reason {
                id: format!("{:x}", hash.finalize()),
                text: bounded,
                source: "hubspot".into(),
                source_property: property.into(),
                application_date: row.value("yingmuri").and_then(date),
                collected_at: None,
                version_id: None,
            });
        }
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: &str, values: &[(&str, Option<&str>)]) -> Record {
        Record {
            id: id.into(),
            properties: values
                .iter()
                .map(|(key, value)| ((*key).into(), value.map(str::to_owned)))
                .collect(),
        }
    }
    fn extract_rows(rows: &[Record]) -> Reasons {
        extract("30", rows, "2026-10-05T00:00:00Z".into())
    }
    #[test]
    fn source_counts_conserve_missing_blank_and_nonblank_without_person_fields() {
        let a = row(
            "50",
            &[
                ("oubodouki", Some(" Flexible hours ")),
                ("ouboriyuu_baitaikisai", Some(" \n ")),
                ("ouboriyuu_hiaringu", None),
                ("yingmuri", Some("2026-10-03")),
                ("email", Some("fictional@example.invalid")),
            ],
        );
        let b = row(
            "51",
            &[
                ("ouboriyuu_hiaringu", Some("Flexible hours")),
                ("yingmuri", Some("invalid")),
            ],
        );
        let reasons = extract_rows(&[a.clone(), a, b]);
        assert_eq!(
            (reasons.total_applicants, reasons.total_source_values),
            (2, 6)
        );
        assert_eq!(
            (reasons.missing, reasons.blank, reasons.items.len()),
            (3, 1, 2)
        );
        assert_eq!(reasons.items[0].text, "Flexible hours");
        assert_eq!(
            reasons.items[0].application_date.as_deref(),
            Some("2026-10-03")
        );
        assert_eq!(reasons.items[1].application_date, None);
        assert!(reasons
            .items
            .iter()
            .all(|item| item.version_id.is_none() && item.collected_at.is_none()));
        let json = serde_json::to_value(reasons).unwrap();
        assert!(!json.to_string().contains("fictional@example.invalid"));
        assert!(!json.to_string().contains("\"email\""));
    }
    #[test]
    fn different_sources_are_distinct_descriptions_and_ids_are_scoped() {
        let rows = [row(
            "50",
            &[
                ("oubodouki", Some("Same")),
                ("ouboriyuu_hiaringu", Some("Same")),
            ],
        )];
        let reasons = extract_rows(&rows);
        assert_eq!(reasons.items.len(), 2);
        assert_ne!(reasons.items[0].id, reasons.items[1].id);
        assert_eq!(reasons.items[0].id.len(), 64);
        assert_eq!(reasons.items[0].id, extract_rows(&rows).items[0].id);
        assert_ne!(
            reasons.items[0].id,
            extract("31", &rows, reasons.fetched_at).items[0].id
        );
    }
    #[test]
    fn unicode_length_and_item_caps_do_not_change_source_observation_counts() {
        let long = "働".repeat(MAX_TEXT_CHARS + 1);
        let rows: Vec<_> = (0..MAX_ITEMS + 1)
            .map(|index| row(&index.to_string(), &[("oubodouki", Some(&long))]))
            .collect();
        let reasons = extract_rows(&rows);
        assert_eq!(reasons.items.len(), MAX_ITEMS);
        assert!(reasons
            .items
            .iter()
            .all(|item| item.text.chars().count() == MAX_TEXT_CHARS));
        assert!(reasons.truncated);
        assert_eq!(reasons.source_counts["oubodouki"].nonblank, MAX_ITEMS + 1);
        assert_eq!(reasons.missing, (MAX_ITEMS + 1) * 2);
    }
    #[test]
    fn verified_empty_read_differs_from_absent_optional_snapshot_field() {
        let reasons = extract_rows(&[]);
        assert!(reasons.available);
        assert_eq!(reasons.total_source_values, 0);
        assert!(reasons.items.is_empty());
        assert!(!reasons.truncated);
    }
}
