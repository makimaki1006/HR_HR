//! Versioned, disposable index cache in the existing audit store. Never log payloads/errors.
use super::*;
use crate::audit::AuditDb;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::io::{Read, Write};

// v2 persists acquisition provenance for reusable ended-listing body summaries.
// v1 cannot distinguish omitted bodies from acquired empty values and is discarded.
const VERSION: i64 = 2;
// Base64 text stays at 1,000,000 bytes, leaving room for the pipeline envelope.
const CHUNK_BYTES: usize = 750_000;
const MAX_PARTS: usize = 128;
const MAX_JSON_BYTES: u64 = 256 * 1024 * 1024;

pub(super) fn media<'de, D: serde::Deserializer<'de>>(d: D) -> Result<&'static str, D::Error> {
    match String::deserialize(d)?.as_str() {
        "hrh" => Ok("hrh"),
        "airwork" => Ok("airwork"),
        _ => Err(serde::de::Error::custom("invalid media")),
    }
}
pub(super) fn title_state<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<&'static str, D::Error> {
    match String::deserialize(d)?.as_str() {
        "set" => Ok("set"),
        "none" => Ok("none"),
        "unset" => Ok("unset"),
        _ => Err(serde::de::Error::custom("invalid title state")),
    }
}

// These values are deliberately skipped in API serialization, but required after restore.
#[derive(Serialize, Deserialize)]
struct PrivateListing {
    applications: Option<usize>,
    body_summary: Option<Summary>,
    perf: Option<home::PerfDigest>,
}
#[derive(Serialize, Deserialize)]
struct PrivateDeal {
    pipeline: Option<String>,
    stage: Option<String>,
    created_at: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct SavedIndex<T> {
    version: i64,
    index: T,
    listings: Vec<PrivateListing>,
    deals: Vec<PrivateDeal>,
}
fn encode(index: &Index) -> Result<(Vec<u8>, usize), ()> {
    let saved = SavedIndex {
        version: VERSION,
        index,
        listings: index
            .records
            .iter()
            .map(|r| PrivateListing {
                applications: r.indexed_application_count,
                body_summary: r.body_summary.clone(),
                perf: r.perf.clone(),
            })
            .collect(),
        deals: index
            .deals
            .iter()
            .map(|d| PrivateDeal {
                pipeline: d.pipeline.clone(),
                stage: d.stage.clone(),
                created_at: d.created_at.clone(),
            })
            .collect(),
    };
    let json = serde_json::to_vec(&saved).map_err(|_| ())?;
    if json.len() as u64 > MAX_JSON_BYTES {
        return Err(());
    }
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&json).map_err(|_| ())?;
    Ok((gzip.finish().map_err(|_| ())?, json.len()))
}
fn decode(bytes: &[u8], now: chrono::DateTime<chrono::Utc>) -> Result<Index, ()> {
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut json)
        .map_err(|_| ())?;
    if json.len() as u64 > MAX_JSON_BYTES {
        return Err(());
    }
    let mut saved: SavedIndex<Index> = serde_json::from_slice(&json).map_err(|_| ())?;
    let built = chrono::DateTime::parse_from_rfc3339(&saved.index.built_at).map_err(|_| ())?;
    let age = now.signed_duration_since(built);
    if saved.version != VERSION
        || age < chrono::Duration::zero()
        || age > chrono::Duration::hours(24)
        || saved.listings.len() != saved.index.records.len()
        || saved.deals.len() != saved.index.deals.len()
    {
        return Err(());
    }
    for (row, private) in saved.index.records.iter_mut().zip(saved.listings) {
        row.indexed_application_count = private.applications;
        row.body_summary = private.body_summary;
        row.perf = private.perf;
    }
    for (deal, private) in saved.index.deals.iter_mut().zip(saved.deals) {
        deal.pipeline = private.pipeline;
        deal.stage = private.stage;
        deal.created_at = private.created_at;
    }
    Ok(saved.index)
}
fn save(audit: &AuditDb, index: &Index) -> Result<(), ()> {
    let (bytes, json_bytes) = encode(index)?;
    let total = bytes.len().div_ceil(CHUNK_BYTES);
    if total == 0 || total > MAX_PARTS {
        return Err(());
    }
    let generation = uuid::Uuid::new_v4().to_string();
    for (part, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
        let payload = STANDARD.encode(chunk);
        audit.turso().execute(
            "INSERT INTO job_copy_index_snapshots (generation,version,built_at,part,total,payload) VALUES (?,?,?,?,?,?)",
            &[&generation, &VERSION, &index.built_at, &(part as i64), &(total as i64), &payload],
        ).map_err(|_| ())?;
    }
    tracing::info!(
        json_bytes,
        gzip_bytes = bytes.len(),
        parts = total,
        "job-copy index snapshot saved"
    );
    Ok(())
}
fn load(audit: &AuditDb) -> Result<Option<Index>, ()> {
    // Incomplete generations are ignored. Fetch each payload separately to bound HTTP responses too.
    let rows = audit.turso().query(
        "SELECT generation, total, built_at FROM job_copy_index_snapshots WHERE version = ? GROUP BY generation HAVING COUNT(*) = MAX(total) AND MIN(total) = MAX(total) AND MIN(part) = 0 AND MAX(part) = MAX(total)-1 ORDER BY built_at DESC LIMIT 1",
        &[&VERSION],
    ).map_err(|_| ())?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let generation = row["generation"].as_str().ok_or(())?.to_owned();
    let total = row["total"].as_i64().ok_or(())?;
    if !(1..=MAX_PARTS as i64).contains(&total) {
        return Err(());
    }
    let built = chrono::DateTime::parse_from_rfc3339(row["built_at"].as_str().ok_or(())?)
        .map_err(|_| ())?;
    let age = chrono::Utc::now().signed_duration_since(built);
    if age < chrono::Duration::zero() || age > chrono::Duration::hours(24) {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    for part in 0..total {
        let rows = audit.turso().query(
            "SELECT payload FROM job_copy_index_snapshots WHERE generation = ? AND part = ? AND version = ?",
            &[&generation, &part, &VERSION],
        ).map_err(|_| ())?;
        let text = rows.first().and_then(|r| r["payload"].as_str()).ok_or(())?;
        if text.len() > 1_000_000 {
            return Err(());
        }
        bytes.extend(STANDARD.decode(text).map_err(|_| ())?);
    }
    let index = decode(&bytes, chrono::Utc::now())?;
    if chrono::DateTime::parse_from_rfc3339(&index.built_at).map_err(|_| ())? != built {
        return Err(());
    }
    Ok(Some(index))
}
pub(super) async fn restore(cache: &Cache, audit: Option<AuditDb>) {
    let Some(audit) = audit else {
        return;
    };
    match tokio::task::spawn_blocking(move || load(&audit)).await {
        Ok(Ok(Some(index))) => {
            let mut state = cache.state.write().await;
            state.snapshot = Some(Arc::new(index));
            state.refreshing = true;
        }
        Ok(Ok(None)) => {}
        _ => tracing::warn!("job-copy index snapshot restore failed"),
    }
}
pub(super) async fn persist(index: Arc<Index>, audit: Option<AuditDb>) {
    let Some(audit) = audit else {
        return;
    };
    if !matches!(
        tokio::task::spawn_blocking(move || save(&audit, &index)).await,
        Ok(Ok(()))
    ) {
        tracing::warn!("job-copy index snapshot save failed");
    }
}

#[cfg(test)]
#[path = "listing_snapshot_tests.rs"]
mod tests;

#[cfg(test)]
pub(super) async fn test_round_trip(index: Index) -> Index {
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    tokio::task::spawn_blocking(move || {
        save(&audit, &index).unwrap();
        load(&audit).unwrap().unwrap()
    })
    .await
    .unwrap()
}
