//! Optional read-only bridge to a privately configured capture artifact.
//! This is an evaluation input, not a new CRM database or production history store.
use crate::job_copy_date::{self, ApplicantRecord, AttributeDimension, Observation};
use serde_json::{json, Value};

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
    let mut per_version = serde_json::Map::new();
    for version in versions {
        let mut dimensions = serde_json::Map::new();
        for (name, dimension) in [
            ("gender", AttributeDimension::Gender),
            ("age", AttributeDimension::Age),
            ("prefecture", AttributeDimension::Prefecture),
            ("municipality", AttributeDimension::Municipality),
        ] {
            dimensions.insert(
                name.into(),
                json!(job_copy_date::distribution_for_version(
                    Some(applications),
                    &aggregation,
                    &version,
                    dimension
                )),
            );
        }
        per_version.insert(version.clone(),json!({"count":aggregation.per_version.get(&version).copied().unwrap_or(0),"dimensions":dimensions}));
    }
    // Do not expose application IDs or row-level personal attributes to the browser.
    Ok(
        json!({"rule_version":aggregation.rule_version,"total":aggregation.total_unique_records,"unknown":aggregation.unknown_records,
        "by_version":per_version,"daily_representatives":index.representatives,"excluded_observations":index.excluded,
        "basis":"変更検知日を基準に集計。媒体生成日が不明の観測は鮮度未確認。"}),
    )
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
}
