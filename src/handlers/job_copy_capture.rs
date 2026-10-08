//! Optional read-only bridge to a privately configured capture artifact.
//! This is an evaluation input, not a new CRM database or production history store.
use crate::geo::applicant_area;
use crate::job_copy_date::{self, ApplicantRecord, AttributeDimension, Observation};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub async fn capture_for_hr_job(
    media_job_id: Option<&str>,
    listing_id: &str,
    shop_id: Option<&str>,
) -> Result<Option<Value>, &'static str> {
    let Some(media_job_id) = media_job_id.filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    let Ok(path) = std::env::var("JOB_COPY_CAPTURE_PATH") else {
        return Ok(None);
    };
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| "capture_file_unavailable")?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err("capture_file_invalid");
    }
    let raw = tokio::fs::read(&path)
        .await
        .map_err(|_| "capture_file_unavailable")?;
    if raw.len() > 32 * 1024 * 1024 {
        return Err("capture_file_invalid");
    }
    let mut bundle: Value = serde_json::from_slice(&raw).map_err(|_| "capture_file_invalid")?;
    if bundle["schemaVersion"] != 1 {
        return Err("capture_schema_invalid");
    }
    let jobs = bundle["jobs"].as_array().ok_or("capture_schema_invalid")?;
    let matches: Vec<_> = jobs
        .iter()
        .filter(|job| {
            job["media"] == "HRハッカー"
                && job["mediaJobId"] == media_job_id
                && job["hubspotListingId"] == listing_id
                && shop_id.is_some_and(|id| job["shopId"] == id)
        })
        .cloned()
        .collect();
    if matches.len() > 1 {
        return Err("capture_job_ambiguous");
    }
    if matches.is_empty() {
        return Ok(None);
    }
    bundle["jobs"] = json!(matches);
    Ok(Some(bundle))
}

pub fn dated_comparison(
    bundle: &Value,
    applications: &[ApplicantRecord],
) -> Result<Value, &'static str> {
    dated_comparison_with_groups(bundle, applications).map(|(comparison, _)| comparison)
}

/// The comparison, and each attributed application's version (application ID -> version ID) for
/// job_copy_live::summarize_grouped. The IDs never leave the server.
pub fn dated_comparison_with_groups(
    bundle: &Value,
    applications: &[ApplicantRecord],
) -> Result<(Value, BTreeMap<String, String>), &'static str> {
    let job = bundle["jobs"]
        .as_array()
        .and_then(|jobs| jobs.first())
        .ok_or("capture_schema_invalid")?;
    let mut observations = Vec::new();
    let mut versions = Vec::new();
    if let Some(history) = job["history"].as_array() {
        for entry in history {
            let id = entry["id"].as_str().ok_or("capture_schema_invalid")?;
            observations.push(observation(
                id,
                entry,
                entry["capturedAt"]
                    .as_str()
                    .ok_or("capture_schema_invalid")?,
            )?);
            versions.push(id.to_owned());
        }
    }
    let captured = bundle["capturedAt"]
        .as_str()
        .ok_or("capture_schema_invalid")?;
    let id = format!(
        "capture-{}-{captured}",
        job["id"].as_str().ok_or("capture_schema_invalid")?
    );
    observations.push(observation(&id, job, captured)?);
    versions.push(id);
    let index = job_copy_date::build_daily_index(
        &observations,
        job_copy_date::date_in_jst(&chrono::Utc::now()),
    );
    let aggregation = job_copy_date::aggregate_applications(&index, Some(applications))
        .ok_or("application_source_unavailable")?;
    let groups: BTreeMap<String, String> = aggregation
        .attributions
        .iter()
        .filter_map(|item| Some((item.application_id.clone(), item.version_id.clone()?)))
        .collect();
    let hidden_areas = protected_areas(applications, &groups);
    let mut per_version = serde_json::Map::new();
    for version in versions {
        let mut dimensions = serde_json::Map::new();
        for (name, dimension) in [
            ("gender", AttributeDimension::Gender),
            ("age", AttributeDimension::Age),
            ("prefecture", AttributeDimension::Prefecture),
            ("municipality", AttributeDimension::Municipality),
        ] {
            let distribution = job_copy_date::distribution_for_version(
                Some(applications),
                &aggregation,
                &version,
                dimension,
            );
            let distribution = match dimension {
                // A version's areas come from the hidden areas (version x gender x age x area
                // of 3 or more), never from the raw ones.
                AttributeDimension::Prefecture | AttributeDimension::Municipality => distribution
                    .map(|distribution| {
                        area_distribution(
                            &groups,
                            &hidden_areas,
                            &version,
                            dimension == AttributeDimension::Municipality,
                            distribution.denominator,
                        )
                    }),
                _ => distribution,
            };
            dimensions.insert(name.into(), json!(distribution));
        }
        per_version.insert(version.clone(),json!({"count":aggregation.per_version.get(&version).copied().unwrap_or(0),"dimensions":dimensions}));
    }
    // Do not expose application IDs or row-level personal attributes to the browser.
    Ok((
        json!({"rule_version":aggregation.rule_version,"total":aggregation.total_unique_records,"unknown":aggregation.unknown_records,
        "by_version":per_version,"daily_representatives":index.representatives,"excluded_observations":index.excluded,
        "area_rule":super::job_copy_live::VERSION_AREA_RULE,
        "basis":"変更検知日を基準に集計。媒体生成日が不明の観測は鮮度未確認。"}),
        groups,
    ))
}

/// Each application's (都道府県, 市区町村) after hiding: by application ID, first row per ID.
fn protected_areas(
    applications: &[ApplicantRecord],
    groups: &BTreeMap<String, String>,
) -> BTreeMap<String, (String, String)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut ids = Vec::new();
    let mut keys = Vec::new();
    for application in applications {
        if !seen.insert(application.application_id.as_str()) {
            continue;
        }
        let attributes = &application.attributes;
        let area = applicant_area::round_area(
            attributes.prefecture.as_deref(),
            attributes.municipality.as_deref(),
        );
        ids.push(application.application_id.clone());
        keys.push(applicant_area::ApplicantKey {
            group: groups
                .get(&application.application_id)
                .cloned()
                .unwrap_or_default(),
            gender: attributes
                .gender
                .as_deref()
                .map(str::trim)
                .filter(|gender| !gender.is_empty())
                .unwrap_or("不明")
                .to_owned(),
            age: super::job_copy_live::age_band(attributes.age.map(u32::from)),
            prefecture: applicant_area::prefecture_label(&area),
            municipality: applicant_area::municipality_label(&area),
            count: 1,
        });
    }
    ids.into_iter()
        .zip(applicant_area::protect_applicant_keys(&keys))
        .collect()
}

fn area_distribution(
    groups: &BTreeMap<String, String>,
    hidden_areas: &BTreeMap<String, (String, String)>,
    version: &str,
    municipality: bool,
    denominator: usize,
) -> job_copy_date::AttributeDistribution {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (id, group) in groups {
        if group != version {
            continue;
        }
        if let Some((prefecture, city)) = hidden_areas.get(id) {
            let label = if municipality { city } else { prefecture };
            *counts.entry(label.clone()).or_default() += 1;
        }
    }
    job_copy_date::AttributeDistribution {
        denominator,
        categories: counts
            .into_iter()
            .map(|(category, count)| job_copy_date::AttributeCount {
                category,
                count,
                percentage: (denominator > 0).then(|| count as f64 / denominator as f64 * 100.0),
            })
            .collect(),
    }
}
fn observation(id: &str, entry: &Value, captured: &str) -> Result<Observation, &'static str> {
    Ok(Observation {
        observation_id: id.into(),
        version_id: id.into(),
        observed_at: chrono::DateTime::parse_from_rfc3339(captured)
            .map_err(|_| "capture_date_invalid")?,
        quality_valid: entry["body"]
            .as_str()
            .is_some_and(|body| !body.trim().is_empty()),
        source_generated_date: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_copy_date::{ApplicantAttributes, ApplicationDate};
    #[test]
    fn missing_observation_day_is_not_allocated() {
        let bundle = json!({"schemaVersion":1,"capturedAt":"2026-10-03T01:00:00Z","jobs":[{"id":"hrh-1","body":"B","history":[{"id":"old","capturedAt":"2026-10-01T01:00:00Z","body":"A"}]}]});
        let apps: Vec<_> = ["2026-10-01", "2026-10-02", "2026-10-03"]
            .iter()
            .enumerate()
            .map(|(i, date)| ApplicantRecord {
                application_id: i.to_string(),
                application_date: ApplicationDate::DateOnly {
                    date: chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
                },
                listing_unambiguous: true,
                attributes: ApplicantAttributes::default(),
            })
            .collect();
        let result = dated_comparison(&bundle, &apps).unwrap();
        assert_eq!(result["total"], 3);
        assert_eq!(result["unknown"], 1);
        assert_eq!(result["by_version"]["old"]["count"], 1);
        assert_eq!(
            result["by_version"]["old"]["dimensions"]["gender"]["categories"][0]["count"],
            1
        );
        assert!(!result.to_string().contains("application_id"));
    }

    #[test]
    fn a_version_in_one_city_does_not_name_the_city_next_to_single_gender_and_age() {
        let bundle = json!({"schemaVersion":1,"capturedAt":"2026-10-03T01:00:00Z","jobs":[{"id":"hrh-1","body":"B","history":[{"id":"old","capturedAt":"2026-10-01T01:00:00Z","body":"A"}]}]});
        let app = |id: &str, day: &str, gender: &str, age: u8| ApplicantRecord {
            application_id: id.into(),
            application_date: ApplicationDate::DateOnly {
                date: chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap(),
            },
            listing_unambiguous: true,
            attributes: ApplicantAttributes {
                gender: Some(gender.into()),
                age: Some(age),
                prefecture: Some("大分県".into()),
                municipality: Some("大分市".into()),
            },
        };
        // version "old" (10-01): 3 applications in 大分市, each a different gender × age.
        let apps = vec![
            app("1", "2026-10-01", "女性", 24),
            app("2", "2026-10-01", "男性", 35),
            app("3", "2026-10-01", "女性", 47),
        ];
        let (result, groups) = dated_comparison_with_groups(&bundle, &apps).unwrap();
        assert_eq!(
            result["area_rule"],
            super::super::job_copy_live::VERSION_AREA_RULE
        );
        assert_eq!(groups.len(), 3);
        let version = &result["by_version"]["old"]["dimensions"];
        assert_eq!(
            version["municipality"]["categories"],
            json!([{"category": "その他", "count": 3, "percentage": 100.0}])
        );
        assert_eq!(
            version["prefecture"]["categories"],
            json!([{"category": "その他", "count": 3, "percentage": 100.0}])
        );
        assert_eq!(version["gender"]["denominator"], 3);
        assert!(!result.to_string().contains("大分市"));
        // 3 applications with the same gender and age keep the city.
        let apps = vec![
            app("1", "2026-10-01", "女性", 24),
            app("2", "2026-10-01", "女性", 25),
            app("3", "2026-10-01", "女性", 27),
        ];
        let result = dated_comparison(&bundle, &apps).unwrap();
        assert_eq!(
            result["by_version"]["old"]["dimensions"]["municipality"]["categories"],
            json!([{"category": "大分県大分市", "count": 3, "percentage": 100.0}])
        );
    }
}
