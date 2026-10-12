//! Operator-run AirWork image linking. Never creates a HubSpot property.
use rust_dashboard::handlers::{
    job_copy_drive::DriveReader,
    job_copy_image_bridge::{ImageBridge, Pointer},
    job_copy_live::JobReadService,
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() {
    if let Err(code) = run().await {
        eprintln!("{code}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let check_only = args.len() == 3 && args[2] == "--check";
    if !check_only && (args.len() != 5 || args[4] != "--apply") {
        return Err("usage_env_key_manifest_results_output_apply");
    }
    dotenvy::from_path(&args[0]).map_err(|_| "env_unavailable")?;
    let token = std::env::var("HUBSPOT_ACCESS_TOKEN").map_err(|_| "hubspot_not_configured")?;
    let key = std::fs::read_to_string(&args[1]).map_err(|_| "key_unavailable")?;
    let reader = Arc::new(DriveReader::new_with_key(&key).map_err(|_| "drive_not_configured")?);
    let jobs = Arc::new(JobReadService::new(token.clone()).map_err(|_| "hubspot_not_configured")?);
    let bridge = ImageBridge::new(token, reader, jobs)?;
    bridge.require_property().await?;
    if check_only {
        return Ok(());
    }
    let raw = std::fs::read(&args[2]).map_err(|_| "pending_unavailable")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("pending_too_large");
    }
    let pending: Value = serde_json::from_slice(&raw).map_err(|_| "pending_invalid")?;
    let rows = pending["results"]
        .as_array()
        .filter(|r| !r.is_empty() && r.len() <= 59)
        .ok_or("pending_invalid")?;
    let mut prepared = vec![];
    let mut seen = BTreeSet::new();
    for row in rows {
        if row["readback_hash_matches"] != true {
            return Err("manifest_not_verified");
        }
        let listing = row["listing_id"].as_str().ok_or("listing_missing")?;
        let company = row["company_ids"][0].as_str().ok_or("company_missing")?;
        if !seen.insert(listing) {
            return Err("duplicate_listing");
        }
        let pointer = Pointer {
            file_id: row["manifest_file_id"]
                .as_str()
                .ok_or("manifest_missing")?
                .into(),
            sha256: row["sha256"].as_str().ok_or("hash_missing")?.into(),
            observed_at: row["observed_at"].as_str().ok_or("date_missing")?.into(),
        };
        pointer.validate()?;
        prepared.push((company, listing, pointer));
    }
    let output = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&output).map_err(|_| "output_unavailable")?;
    let mut results = vec![];
    for (company, listing, pointer) in prepared {
        let status = bridge.publish_airwork(company, listing, &pointer).await?;
        results.push(json!({"listing_id":listing,"status":status}));
        let temporary = output.join("link-results.tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec(&json!({"results":results})).map_err(|_| "serialize_failed")?,
        )
        .map_err(|_| "output_unavailable")?;
        std::fs::rename(temporary, output.join("link-results.json"))
            .map_err(|_| "output_unavailable")?;
        println!("{}", json!({"verifiedJobs":results.len()}));
    }
    Ok(())
}
