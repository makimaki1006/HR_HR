//! Pure JST day-based observation and application attribution.
//!
//! Callers supply verified source-quality and relationship results. This module
//! never fetches records, writes history, or invents a publication timestamp.
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ATTRIBUTION_RULE_VERSION: &str = "jst-last-valid-observation-v1";

pub fn date_in_jst<Tz: TimeZone>(timestamp: &DateTime<Tz>) -> NaiveDate {
    timestamp
        .with_timezone(&FixedOffset::east_opt(9 * 60 * 60).expect("valid JST offset"))
        .date_naive()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub observation_id: String,
    pub version_id: String,
    pub observed_at: DateTime<FixedOffset>,
    /// True only after body, source scope and original-file quality checks.
    /// Image acquisition failure alone does not make the body invalid.
    pub quality_valid: bool,
    pub source_generated_date: Option<NaiveDate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFreshness {
    ConfirmedSameDay,
    Unverified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyRepresentative {
    pub date: NaiveDate,
    pub observation_id: String,
    pub version_id: String,
    pub source_freshness: SourceFreshness,
    pub provisional: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationExclusion {
    QualityInvalid,
    StaleExport,
    SourceDateInFuture,
    ObservationDateInFuture,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedObservation {
    pub observation_id: String,
    pub reason: ObservationExclusion,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyIndex {
    pub rule_version: String,
    /// Full chronological history, including invalid captures and A -> B -> A.
    pub observations: Vec<Observation>,
    /// Successful-day set; gaps are intentionally never carried forward.
    pub representatives: BTreeMap<NaiveDate, DailyRepresentative>,
    pub excluded: Vec<ExcludedObservation>,
    /// Different final versions observed at exactly the same timestamp.
    pub conflicting_dates: Vec<NaiveDate>,
}

/// Build one listing's day index. Other listings must be processed separately.
pub fn build_daily_index(observations: &[Observation], today_jst: NaiveDate) -> DailyIndex {
    let mut index = DailyIndex {
        rule_version: ATTRIBUTION_RULE_VERSION.to_owned(),
        observations: observations.to_vec(),
        representatives: BTreeMap::new(),
        excluded: Vec::new(),
        conflicting_dates: Vec::new(),
    };
    index.observations.sort_by_key(|item| item.observed_at);
    let mut valid: BTreeMap<NaiveDate, Vec<&Observation>> = BTreeMap::new();
    for observation in &index.observations {
        let day = date_in_jst(&observation.observed_at);
        let reason = if !observation.quality_valid {
            Some(ObservationExclusion::QualityInvalid)
        } else if day > today_jst {
            Some(ObservationExclusion::ObservationDateInFuture)
        } else if observation
            .source_generated_date
            .is_some_and(|source| source < day)
        {
            Some(ObservationExclusion::StaleExport)
        } else if observation
            .source_generated_date
            .is_some_and(|source| source > day)
        {
            Some(ObservationExclusion::SourceDateInFuture)
        } else {
            None
        };
        if let Some(reason) = reason {
            index.excluded.push(ExcludedObservation {
                observation_id: observation.observation_id.clone(),
                reason,
            });
        } else {
            valid.entry(day).or_default().push(observation);
        }
    }
    for (date, candidates) in valid {
        let last = candidates.last().expect("nonempty observation group");
        if candidates
            .iter()
            .any(|item| item.observed_at == last.observed_at && item.version_id != last.version_id)
        {
            index.conflicting_dates.push(date);
            continue;
        }
        index.representatives.insert(
            date,
            DailyRepresentative {
                date,
                observation_id: last.observation_id.clone(),
                version_id: last.version_id.clone(),
                source_freshness: if last.source_generated_date.is_some() {
                    SourceFreshness::ConfirmedSameDay
                } else {
                    SourceFreshness::Unverified
                },
                provisional: date == today_jst,
            },
        );
    }
    index
}

/// No UTC midnight is manufactured for a date-only application.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "precision", rename_all = "snake_case")]
pub enum ApplicationDate {
    DateOnly {
        date: NaiveDate,
    },
    RealTimestamp {
        timestamp: DateTime<FixedOffset>,
    },
    /// A HubSpot workflow's synthetic hs_appointment_start is not an event time.
    SyntheticTimestamp {
        timestamp: DateTime<FixedOffset>,
    },
    Unknown,
}

impl ApplicationDate {
    pub fn jst_date(&self) -> Option<NaiveDate> {
        match self {
            Self::DateOnly { date } => Some(*date),
            Self::RealTimestamp { timestamp } => Some(date_in_jst(timestamp)),
            Self::SyntheticTimestamp { .. } | Self::Unknown => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicantAttributes {
    pub gender: Option<String>,
    pub age: Option<u8>,
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicantRecord {
    pub application_id: String,
    pub application_date: ApplicationDate,
    /// Determined by the association adapter, never by job-title similarity.
    pub listing_unambiguous: bool,
    pub attributes: ApplicantAttributes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownAttributionReason {
    AmbiguousListing,
    MissingApplicationDate,
    SyntheticApplicationTimestamp,
    MissingSuccessfulObservation,
    ConflictingDailyObservations,
    ConflictingDuplicateRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    pub application_id: String,
    pub application_date: Option<NaiveDate>,
    pub version_id: Option<String>,
    pub source_freshness: Option<SourceFreshness>,
    pub unknown_reason: Option<UnknownAttributionReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationAggregation {
    pub rule_version: String,
    pub total_unique_records: usize,
    pub duplicate_rows: usize,
    pub per_version: BTreeMap<String, usize>,
    pub unknown_records: usize,
    pub attributions: Vec<Attribution>,
}

/// Input is restricted to the same listing as `index` by the association adapter.
/// None means source unavailable; Some(empty) means an acquired zero-record set.
pub fn aggregate_applications(
    index: &DailyIndex,
    applications: Option<&[ApplicantRecord]>,
) -> Option<ApplicationAggregation> {
    let applications = applications?;
    let mut grouped: BTreeMap<&str, Vec<&ApplicantRecord>> = BTreeMap::new();
    for application in applications {
        grouped
            .entry(&application.application_id)
            .or_default()
            .push(application);
    }
    let mut result = ApplicationAggregation {
        rule_version: ATTRIBUTION_RULE_VERSION.to_owned(),
        total_unique_records: grouped.len(),
        duplicate_rows: applications.len() - grouped.len(),
        per_version: BTreeMap::new(),
        unknown_records: 0,
        attributions: Vec::with_capacity(grouped.len()),
    };
    for (_, rows) in grouped {
        let application = rows[0];
        let date = application.application_date.jst_date();
        let reason = if rows.iter().any(|row| **row != *application) {
            Some(UnknownAttributionReason::ConflictingDuplicateRecord)
        } else if !application.listing_unambiguous {
            Some(UnknownAttributionReason::AmbiguousListing)
        } else if matches!(
            application.application_date,
            ApplicationDate::SyntheticTimestamp { .. }
        ) {
            Some(UnknownAttributionReason::SyntheticApplicationTimestamp)
        } else if date.is_none() {
            Some(UnknownAttributionReason::MissingApplicationDate)
        } else if index
            .conflicting_dates
            .contains(&date.expect("checked application date"))
        {
            Some(UnknownAttributionReason::ConflictingDailyObservations)
        } else if !index
            .representatives
            .contains_key(&date.expect("checked application date"))
        {
            Some(UnknownAttributionReason::MissingSuccessfulObservation)
        } else {
            None
        };
        let representative = if reason.is_none() {
            date.and_then(|day| index.representatives.get(&day))
        } else {
            None
        };
        if let Some(representative) = representative {
            *result
                .per_version
                .entry(representative.version_id.clone())
                .or_default() += 1;
        } else {
            result.unknown_records += 1;
        }
        result.attributions.push(Attribution {
            application_id: application.application_id.clone(),
            application_date: date,
            version_id: representative.map(|item| item.version_id.clone()),
            source_freshness: representative.map(|item| item.source_freshness),
            unknown_reason: reason,
        });
    }
    Some(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeDimension {
    Gender,
    Age,
    Prefecture,
    Municipality,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttributeCount {
    pub category: String,
    pub count: usize,
    pub percentage: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttributeDistribution {
    pub denominator: usize,
    pub categories: Vec<AttributeCount>,
}

fn supplied(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn attribute_category(attributes: &ApplicantAttributes, dimension: AttributeDimension) -> String {
    match dimension {
        AttributeDimension::Gender => supplied(&attributes.gender).unwrap_or("不明").to_owned(),
        AttributeDimension::Prefecture => supplied(&attributes.prefecture)
            .unwrap_or("不明")
            .to_owned(),
        AttributeDimension::Municipality => match (
            supplied(&attributes.prefecture),
            supplied(&attributes.municipality),
        ) {
            (None, None) => "不明".to_owned(),
            (prefecture, municipality) => format!(
                "{} / {}",
                prefecture.unwrap_or("都道府県不明"),
                municipality.unwrap_or("市区町村不明")
            ),
        },
        AttributeDimension::Age => match attributes.age {
            None => "不明".to_owned(),
            Some(0..=19) => "19歳以下".to_owned(),
            Some(60..=u8::MAX) => "60歳以上".to_owned(),
            Some(age) => format!("{}代", age / 10 * 10),
        },
    }
}

/// Build a denominator from attributed, deduplicated application IDs only.
pub fn distribution_for_version(
    applications: Option<&[ApplicantRecord]>,
    aggregation: &ApplicationAggregation,
    version_id: &str,
    dimension: AttributeDimension,
) -> Option<AttributeDistribution> {
    let applications = applications?;
    let ids: BTreeSet<&str> = aggregation
        .attributions
        .iter()
        .filter(|item| item.version_id.as_deref() == Some(version_id))
        .map(|item| item.application_id.as_str())
        .collect();
    let mut seen = BTreeSet::new();
    let mut counts = BTreeMap::new();
    for application in applications {
        if ids.contains(application.application_id.as_str())
            && seen.insert(application.application_id.as_str())
        {
            *counts
                .entry(attribute_category(&application.attributes, dimension))
                .or_insert(0usize) += 1;
        }
    }
    let denominator = counts.values().sum();
    if denominator != ids.len() {
        // Do not combine a report's attribution set with a different or partial
        // applicant snapshot and silently lower its denominator.
        return None;
    }
    Some(AttributeDistribution {
        denominator,
        categories: counts
            .into_iter()
            .map(|(category, count)| AttributeCount {
                category,
                count,
                percentage: if denominator == 0 {
                    None
                } else {
                    Some(count as f64 / denominator as f64 * 100.0)
                },
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests;
