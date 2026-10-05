//! Operator-run limited integration validation. No server/DB startup; writes only with --apply.
use rust_dashboard::handlers::{
    job_copy_drive::DriveReader,
    job_copy_image_bridge::{ImageBridge, Pointer},
    job_copy_live::{hydrate_drive_images, JobReadService},
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
    if args.len() < 4 || args.len() > 5 || args.get(4).is_some_and(|x| x != "--apply") {
        return Err("usage_env_key_manifest_results_output_dir_optional_apply");
    }
    dotenvy::from_path(&args[0]).map_err(|_| "env_unavailable")?;
    let token = std::env::var("HUBSPOT_ACCESS_TOKEN").map_err(|_| "hubspot_not_configured")?;
    let key = std::fs::read_to_string(&args[1]).map_err(|_| "key_unavailable")?;
    let reader = Arc::new(DriveReader::new_with_key(&key).map_err(|_| "drive_not_configured")?);
    let jobs = Arc::new(JobReadService::new(token.clone()).map_err(|_| "hubspot_not_configured")?);
    let bridge = ImageBridge::new(token, reader, jobs)?;
    let raw = std::fs::read(&args[2]).map_err(|_| "pending_unavailable")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("pending_too_large");
    }
    let pending: Value = serde_json::from_slice(&raw).map_err(|_| "pending_invalid")?;
    let rows = pending["results"]
        .as_array()
        .filter(|r| !r.is_empty() && r.len() <= 59)
        .ok_or("pending_invalid")?;
    let moc_path = std::env::var("JOB_COPY_MOC_PATH")
        .unwrap_or_else(|_| "data/job-copy-local/real-moc.json".into());
    let moc_bytes = std::fs::read(moc_path).map_err(|_| "moc_unavailable")?;
    if moc_bytes.len() > 32 * 1024 * 1024 {
        return Err("moc_too_large");
    }
    let mut moc: Value = serde_json::from_slice(&moc_bytes).map_err(|_| "moc_invalid")?;
    let moc_jobs = moc["capture_bundle"]["jobs"]
        .as_array()
        .ok_or("moc_invalid")?;
    let mut pending_ids = BTreeSet::new();
    for row in rows {
        let listing = row["listing_id"].as_str().ok_or("listing_missing")?;
        let company = row["company_ids"][0].as_str().ok_or("company_missing")?;
        if !pending_ids.insert(listing)
            || moc_jobs
                .iter()
                .filter(|job| {
                    job["hubspotListingId"].as_str() == Some(listing)
                        && job["companyIds"]
                            .as_array()
                            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(company)))
                })
                .count()
                != 1
        {
            return Err("pending_moc_identity_mismatch");
        }
        Pointer {
            file_id: row["manifest_file_id"]
                .as_str()
                .ok_or("manifest_missing")?
                .into(),
            sha256: row["sha256"].as_str().ok_or("hash_missing")?.into(),
            observed_at: row["observed_at"].as_str().ok_or("date_missing")?.into(),
        }
        .validate()?;
    }
    let output = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&output).map_err(|_| "output_unavailable")?;
    let mut listings = BTreeSet::new();
    let mut results = vec![];
    if args.len() == 5 {
        bridge.ensure_property().await?;
    }
    for (index, row) in rows.iter().enumerate() {
        let listing = row["listing_id"].as_str().ok_or("listing_missing")?;
        let company = row["company_ids"][0].as_str().ok_or("company_missing")?;
        let pointer = Pointer {
            file_id: row["manifest_file_id"]
                .as_str()
                .ok_or("manifest_missing")?
                .into(),
            sha256: row["sha256"].as_str().ok_or("hash_missing")?.into(),
            observed_at: row["observed_at"].as_str().ok_or("date_missing")?.into(),
        };
        let status = if args.len() == 5 {
            bridge.publish(company, listing, &pointer).await?
        } else {
            "read_only"
        };
        let manifest = bridge.manifest(&pointer).await?;
        let mut images = vec![];
        for image in &manifest.images {
            let start = std::time::Instant::now();
            let (mime, bytes) = bridge
                .image(company, listing, &pointer.file_id, image.slot)
                .await?;
            let ext = match mime.as_str() {
                "image/jpeg" => "jpg",
                "image/png" => "png",
                "image/webp" => "webp",
                _ => return Err("invalid_mime"),
            };
            let path = output.join(format!("verified-image-{index}-{}.{ext}", image.slot));
            std::fs::write(&path, &bytes).map_err(|_| "output_unavailable")?;
            images.push(json!({"slot":image.slot,"bytes":bytes.len(),"mime":mime,"sha256":image.sha256,
                "elapsed_ms":start.elapsed().as_millis(),"path":path,"url":format!("/api/job-copy/image?company_id={company}&listing_id={listing}&manifest_id={}&slot={}",pointer.file_id,image.slot)}));
        }
        listings.insert(listing.to_string());
        results.push(json!({"listing_id":listing,"status":status,"images":images}));
        std::fs::write(
            output.join("link-results.json"),
            serde_json::to_vec_pretty(&json!({"results":results}))
                .map_err(|_| "serialize_failed")?,
        )
        .map_err(|_| "output_unavailable")?;
        println!(
            "{}",
            json!({"job":index+1,"status":status,"verified_images":manifest.images.len()})
        );
    }
    hydrate_drive_images(&mut moc, &bridge, &listings)
        .await
        .map_err(|_| "moc_hydration_failed")?;
    std::fs::write(
        output.join("drive-moc.json"),
        serde_json::to_vec(&moc).map_err(|_| "serialize_failed")?,
    )
    .map_err(|_| "output_unavailable")?;
    println!(
        "{}",
        json!({"jobs":results.len(),"scope":"Rust service integration; no deployed browser authentication tested"})
    );
    Ok(())
}
