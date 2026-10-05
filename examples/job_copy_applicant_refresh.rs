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

fn refresh(source: &Value, mut moc: Value) -> Result<Value, &'static str> {
    let fetched = source["fetched_at"]
        .as_str()
        .filter(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok())
        .ok_or("invalid_source_timestamp")?;
    let raw = source["rows"].as_array().ok_or("missing_rows")?;
    let mut rows = BTreeMap::new();
    for value in raw {
        let row: Record = serde_json::from_value(value.clone()).map_err(|_| "invalid_row")?;
        for field in [
            "yingmuri",
            "seibetsu",
            "nenrei",
            "todoufuken",
            "shikuchouson",
            "oubodouki",
            "ouboriyuu_baitaikisai",
            "ouboriyuu_hiaringu",
        ] {
            if !row.properties.contains_key(field) {
                return Err("source_property_not_requested");
            }
        }
        if rows.insert(row.id.clone(), row).is_some() {
            return Err("duplicate_applicant");
        }
    }
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
        results.push(json!({"listing_id":listing,"summary":job_copy_live::summarize(&records),"dated_comparison":job_copy_capture::dated_comparison(&bundle,&applications)?,"applicant_reasons":applicant_reasons::extract(&listing,&records,fetched.to_owned())}));
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
