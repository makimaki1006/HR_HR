//! Offline refresh from already authorized reads; no network or credentials.
use rust_dashboard::{
    handlers::{
        job_copy_capture,
        job_copy_live::{self, applicant_reasons, Record},
    },
    job_copy_date::{ApplicantAttributes, ApplicantRecord, ApplicationDate},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// The reasons of one job, in the shape that matches the sources the dump was read with. With
/// the option labels read alongside the dump, the chosen categories get their labels; without,
/// the snapshot says the labels were not stored (reopening the screen does not add them).
fn reasons_for(
    listing: &str,
    records: &[Record],
    fetched: &str,
    every_source: bool,
    labels: Option<&applicant_reasons::OptionLabels>,
) -> applicant_reasons::Reasons {
    if every_source {
        applicant_reasons::extract_with_labels(
            listing,
            records,
            fetched.to_owned(),
            labels,
            applicant_reasons::OptionLabelsStatus::NotStored,
        )
    } else {
        applicant_reasons::extract_legacy(listing, records, fetched.to_owned())
    }
}

fn refresh(source: &Value, mut moc: Value) -> Result<Value, &'static str> {
    let fetched = source["fetched_at"]
        .as_str()
        .filter(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok())
        .ok_or("invalid_source_timestamp")?;
    let raw = source["rows"].as_array().ok_or("missing_rows")?;
    let mut rows = BTreeMap::new();
    // A dump read with every reason source gives the current shape. A dump read before the
    // transfer reason and the category selects were added gives the old shape, where those
    // sources are 未取得 (never 記録なし or 0). Rows of one dump must all be read the same way.
    let mut every_source = None;
    for value in raw {
        let row: Record = serde_json::from_value(value.clone()).map_err(|_| "invalid_row")?;
        for field in [
            "yingmuri",
            "seibetsu",
            "nenrei",
            "todoufuken",
            "shikuchouson",
        ]
        .into_iter()
        .chain(applicant_reasons::LEGACY_PROPERTIES)
        {
            if !row.properties.contains_key(field) {
                return Err("source_property_not_requested");
            }
        }
        let added: Vec<bool> = applicant_reasons::PROPERTIES
            .iter()
            .filter(|field| !applicant_reasons::LEGACY_PROPERTIES.contains(field))
            .map(|field| row.properties.contains_key(*field))
            .collect();
        let all = added.iter().all(|read| *read);
        if !all && added.iter().any(|read| *read) {
            return Err("source_property_not_requested");
        }
        if *every_source.get_or_insert(all) != all {
            return Err("mixed_source_properties");
        }
        if rows.insert(row.id.clone(), row).is_some() {
            return Err("duplicate_applicant");
        }
    }
    // Optional: the reply of the property definition read
    // (POST /crm/v3/properties/0-421/batch/read for the two category selects), saved with the dump.
    let labels = match source.get("property_definitions") {
        None => None,
        Some(definitions) => {
            Some(job_copy_live::option_labels(definitions).ok_or("invalid_property_definitions")?)
        }
    };
    let associations = source["associations"]
        .as_object()
        .ok_or("missing_associations")?;
    let jobs = moc["capture_bundle"]["jobs"]
        .as_array()
        .ok_or("missing_jobs")?
        .clone();
    if jobs.len() != associations.len() || jobs.is_empty() || jobs.len() > 59 {
        return Err("listing_coverage_mismatch");
    }
    let mut ownership: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (listing, ids) in associations {
        let mut seen = BTreeSet::new();
        for id in ids.as_array().ok_or("invalid_association")? {
            let id = id.as_str().ok_or("invalid_association_id")?;
            if !rows.contains_key(id) || !seen.insert(id) {
                return Err("association_row_mismatch");
            }
            ownership
                .entry(id.into())
                .or_default()
                .insert(listing.clone());
        }
    }
    if ownership.len() != rows.len() {
        return Err("orphan_applicant");
    }
    let verified_unambiguous: BTreeSet<&str> =
        match source.get("verified_unambiguous_applicant_ids") {
            None => BTreeSet::new(),
            Some(value) => {
                let mut verified = BTreeSet::new();
                for value in value.as_array().ok_or("invalid_verified_associations")? {
                    let id = value.as_str().ok_or("invalid_verified_associations")?;
                    if !rows.contains_key(id) || ownership[id].len() != 1 || !verified.insert(id) {
                        return Err("invalid_verified_associations");
                    }
                }
                verified
            }
        };
    let mut results = Vec::new();
    let mut seen_jobs = BTreeSet::new();
    for job in jobs {
        let listing = job["hubspotListingId"]
            .as_str()
            .ok_or("missing_listing")?
            .to_owned();
        if !seen_jobs.insert(listing.to_owned()) {
            return Err("duplicate_listing");
        }
        let ids = associations
            .get(&listing)
            .and_then(Value::as_array)
            .ok_or("listing_not_read")?;
        let records: Vec<Record> = ids
            .iter()
            .map(|id| rows[id.as_str().expect("validated id")].clone())
            .collect();
        let applications: Vec<_> = records
            .iter()
            .map(|row| {
                let value = |key: &str| {
                    row.properties
                        .get(key)
                        .and_then(Option::as_deref)
                        .filter(|s| !s.trim().is_empty())
                };
                ApplicantRecord {
                    application_id: row.id.clone(),
                    listing_unambiguous: verified_unambiguous.contains(row.id.as_str()),
                    application_date: value("yingmuri")
                        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                        .map(|date| ApplicationDate::DateOnly { date })
                        .unwrap_or(ApplicationDate::Unknown),
                    attributes: ApplicantAttributes {
                        gender: value("seibetsu").map(str::to_owned),
                        age: value("nenrei")
                            .and_then(|s| s.parse::<u8>().ok())
                            .filter(|n| *n <= 120),
                        prefecture: value("todoufuken").map(str::to_owned),
                        municipality: value("shikuchouson").map(str::to_owned),
                    },
                }
            })
            .collect();
        let mut bundle = moc["capture_bundle"].clone();
        bundle["jobs"] = json!([job]);
        // The per-version areas and the job-wide cells are hidden with the same version groups.
        let (comparison, groups) =
            job_copy_capture::dated_comparison_with_groups(&bundle, &applications)?;
        let reasons = reasons_for(
            &listing,
            &records,
            fetched,
            every_source == Some(true),
            labels.as_ref(),
        );
        results.push(json!({"listing_id":listing,"summary":job_copy_live::summarize_grouped(&records,&groups),"dated_comparison":comparison,"applicant_reasons":reasons}));
    }
    moc["capturedAt"] = json!(fetched);
    moc["results"] = json!(results);
    Ok(moc)
}
fn main() {
    if let Err(code) = run() {
        eprintln!("{code}");
        std::process::exit(1)
    }
}
fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: job_copy_applicant_refresh SOURCE_JSON EXISTING_MOC NEW_OUTPUT");
    }
    let output = Path::new(&args[3]);
    if output.exists() {
        return Err("output_exists");
    }
    let read = |path: &str| -> Result<Value, &'static str> {
        serde_json::from_slice(&fs::read(path).map_err(|_| "input_read_failed")?)
            .map_err(|_| "invalid_json")
    };
    let result = refresh(&read(&args[1])?, read(&args[2])?)?;
    let bytes = serde_json::to_vec(&result).map_err(|_| "serialize_failed")?;
    let pending = output.with_extension("pending");
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending)
        .map_err(|_| "pending_create_failed")?;
    let write = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    if write.is_err() {
        let _ = fs::remove_file(&pending);
        return Err("output_write_failed");
    }
    let linked = fs::hard_link(&pending, output);
    let _ = fs::remove_file(&pending);
    linked.map_err(|_| "atomic_output_commit_failed")?;
    println!(
        "jobs={} applicants={} reason_values={}",
        result["results"].as_array().map_or(0, Vec::len),
        result["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["summary"]["total"].as_u64())
            .sum::<u64>(),
        result["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["applicant_reasons"]["items"]
                .as_array()
                .map_or(0, Vec::len))
            .sum::<usize>()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Value {
        let properties: serde_json::Map<_, _> = [
            "yingmuri",
            "seibetsu",
            "nenrei",
            "todoufuken",
            "shikuchouson",
            "oubodouki",
            "ouboriyuu_baitaikisai",
            "ouboriyuu_hiaringu",
        ]
        .into_iter()
        .map(|key| (key.to_owned(), Value::Null))
        .collect();
        json!({"fetched_at":"2026-10-05T00:00:00Z","associations":{"30":["50"]},"rows":[{"id":"50","properties":properties}]})
    }
    fn with_new_sources(mut input: Value) -> Value {
        for field in [
            "genshokumaeshokukaranotenshokuriyuu",
            "ouboriyuukategori_hiaringu",
            "ouboriyuukategori_baitaikisai",
        ] {
            for row in input["rows"].as_array_mut().unwrap() {
                row["properties"][field] = Value::Null;
            }
        }
        input
    }
    fn moc() -> Value {
        json!({"capture_bundle":{"jobs":[{"hubspotListingId":"30"}]}})
    }
    fn rows(input: &Value) -> Vec<Record> {
        input["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| serde_json::from_value(row.clone()).unwrap())
            .collect()
    }
    #[test]
    fn a_dump_without_the_new_sources_gives_the_old_shape_not_zero_counts() {
        // The dump passes the source check (the error comes later, from the empty capture bundle).
        assert_eq!(
            refresh(&source(), moc()).unwrap_err(),
            "capture_schema_invalid"
        );
        let reasons = serde_json::to_value(reasons_for(
            "30",
            &rows(&source()),
            "2026-10-05T00:00:00Z",
            false,
            None,
        ))
        .unwrap();
        assert!(reasons.get("selections").is_none());
        assert_eq!(reasons["source_counts"].as_object().unwrap().len(), 3);
        assert!(reasons["source_counts"]["ouboriyuukategori_hiaringu"].is_null());
        assert_eq!(
            (
                reasons["total_source_values"].as_u64(),
                reasons["missing"].as_u64()
            ),
            (Some(3), Some(3))
        );
    }
    #[test]
    fn a_dump_with_every_source_gives_the_new_shape() {
        let input = with_new_sources(source());
        assert_eq!(
            refresh(&input, moc()).unwrap_err(),
            "capture_schema_invalid"
        );
        let reasons = serde_json::to_value(reasons_for(
            "30",
            &rows(&input),
            "2026-10-05T00:00:00Z",
            true,
            None,
        ))
        .unwrap();
        assert_eq!(reasons["source_counts"].as_object().unwrap().len(), 6);
        assert_eq!(reasons["selections"], json!([]));
        // Without the definitions, the snapshot says the labels were not stored.
        assert_eq!(reasons["option_labels"], "not_stored");
    }
    #[test]
    fn definitions_saved_with_the_dump_name_the_chosen_categories() {
        let mut input = with_new_sources(source());
        input["rows"][0]["properties"]["ouboriyuukategori_hiaringu"] = json!("kyuuyo");
        input["property_definitions"] = json!({"results":[{"name":"ouboriyuukategori_hiaringu",
            "options":[{"value":"kyuuyo","label":"給与"}]}]});
        // The definitions are accepted (the error comes later, from the empty capture bundle).
        assert_eq!(
            refresh(&input, moc()).unwrap_err(),
            "capture_schema_invalid"
        );
        let labels = job_copy_live::option_labels(&input["property_definitions"]).unwrap();
        let reasons = serde_json::to_value(reasons_for(
            "30",
            &rows(&input),
            "2026-10-05T00:00:00Z",
            true,
            Some(&labels),
        ))
        .unwrap();
        assert_eq!(reasons["option_labels"], "read");
        assert_eq!(reasons["selections"][0]["value"], "kyuuyo");
        assert_eq!(reasons["selections"][0]["label"], "給与");
        input["property_definitions"] = json!({"results":[]});
        assert_eq!(
            refresh(&input, moc()).unwrap_err(),
            "invalid_property_definitions"
        );
    }
    #[test]
    fn a_dump_with_only_some_new_sources_or_mixed_rows_is_rejected() {
        let mut input = with_new_sources(source());
        input["rows"][0]["properties"]
            .as_object_mut()
            .unwrap()
            .remove("ouboriyuukategori_baitaikisai");
        assert_eq!(
            refresh(&input, json!({})).unwrap_err(),
            "source_property_not_requested"
        );
        let mut input = source();
        let mut row = with_new_sources(source())["rows"][0].clone();
        row["id"] = json!("51");
        input["rows"].as_array_mut().unwrap().push(row);
        assert_eq!(
            refresh(&input, json!({})).unwrap_err(),
            "mixed_source_properties"
        );
    }
    #[test]
    fn missing_requested_demographics_are_not_converted_to_unknown() {
        let mut input = source();
        input["rows"][0]["properties"]
            .as_object_mut()
            .unwrap()
            .remove("seibetsu");
        assert_eq!(
            refresh(&input, json!({})).unwrap_err(),
            "source_property_not_requested"
        );
    }
    #[test]
    fn duplicate_rows_and_unread_or_orphan_associations_are_rejected() {
        let mut input = source();
        let row = input["rows"][0].clone();
        input["rows"].as_array_mut().unwrap().push(row);
        assert_eq!(
            refresh(&input, json!({})).unwrap_err(),
            "duplicate_applicant"
        );
        let mut input = source();
        input["associations"]["30"] = json!(["51"]);
        let moc = json!({"capture_bundle":{"jobs":[{"hubspotListingId":"30"}]}});
        assert_eq!(
            refresh(&input, moc.clone()).unwrap_err(),
            "association_row_mismatch"
        );
        input["associations"]["30"] = json!([]);
        assert_eq!(refresh(&input, moc).unwrap_err(), "orphan_applicant");
    }
}
