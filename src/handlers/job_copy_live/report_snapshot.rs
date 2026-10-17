//! Disposable derived reports in the existing audit DB. Never log payloads or errors.
use super::*;
use crate::audit::AuditDb;
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use std::io::{Read, Write};

const VERSION: i64 = 1;
const CHUNK_BYTES: usize = 750_000;
const MAX_PARTS: usize = 128;
const MAX_JSON_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Recent,
    Facts,
}
impl Kind {
    fn table(self) -> &'static str {
        match self {
            Self::Recent => "job_copy_recent_snapshots",
            Self::Facts => "job_copy_todo_fact_snapshots",
        }
    }
}
#[derive(Serialize, Deserialize)]
pub(super) struct Saved<T> {
    version: i64,
    pub built_at: DateTime<Utc>,
    pub data: T,
}
fn encode<T: Serialize>(data: &T, built_at: DateTime<Utc>) -> Result<(Vec<u8>, usize), ()> {
    let json = serde_json::to_vec(&Saved {
        version: VERSION,
        built_at,
        data,
    })
    .map_err(|_| ())?;
    if json.len() as u64 > MAX_JSON_BYTES {
        return Err(());
    }
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&json).map_err(|_| ())?;
    Ok((gzip.finish().map_err(|_| ())?, json.len()))
}
fn decode<T: DeserializeOwned>(bytes: &[u8], now: DateTime<Utc>) -> Result<Saved<T>, ()> {
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut json)
        .map_err(|_| ())?;
    if json.len() as u64 > MAX_JSON_BYTES {
        return Err(());
    }
    let saved: Saved<T> = serde_json::from_slice(&json).map_err(|_| ())?;
    // Old days are usable as stale results. Future timestamps and incompatible formats are not.
    if saved.version != VERSION || saved.built_at > now {
        return Err(());
    }
    Ok(saved)
}
fn save<T: Serialize>(
    audit: &AuditDb,
    kind: Kind,
    data: &T,
    built_at: DateTime<Utc>,
) -> Result<(), ()> {
    let (bytes, json_bytes) = encode(data, built_at)?;
    let total = bytes.len().div_ceil(CHUNK_BYTES);
    if total == 0 || total > MAX_PARTS {
        return Err(());
    }
    let generation = uuid::Uuid::new_v4().to_string();
    let built_at = built_at.to_rfc3339();
    for (part, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
        let payload = STANDARD.encode(chunk);
        audit.turso().execute(
            &format!("INSERT INTO {} (generation,version,built_at,part,total,payload) VALUES (?,?,?,?,?,?)", kind.table()),
            &[&generation, &VERSION, &built_at, &(part as i64), &(total as i64), &payload],
        ).map_err(|_| ())?;
    }
    tracing::info!(
        cache = kind.table(),
        json_bytes,
        gzip_bytes = bytes.len(),
        parts = total,
        "job-copy report snapshot saved"
    );
    Ok(())
}
fn load<T: DeserializeOwned>(audit: &AuditDb, kind: Kind) -> Result<Option<Saved<T>>, ()> {
    let rows = audit.turso().query(
        &format!("SELECT generation,total,built_at FROM {} WHERE version = ? GROUP BY generation HAVING COUNT(*) = MAX(total) AND MIN(total) = MAX(total) AND MIN(part) = 0 AND MAX(part) = MAX(total)-1 ORDER BY built_at DESC LIMIT 1", kind.table()),
        &[&VERSION],
    ).map_err(|_| ())?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let generation = row["generation"].as_str().ok_or(())?;
    let total = row["total"].as_i64().ok_or(())?;
    if !(1..=MAX_PARTS as i64).contains(&total) {
        return Err(());
    }
    let built =
        DateTime::parse_from_rfc3339(row["built_at"].as_str().ok_or(())?).map_err(|_| ())?;
    let mut bytes = Vec::new();
    for part in 0..total {
        let rows = audit
            .turso()
            .query(
                &format!(
                    "SELECT payload FROM {} WHERE generation = ? AND part = ? AND version = ?",
                    kind.table()
                ),
                &[&generation, &part, &VERSION],
            )
            .map_err(|_| ())?;
        let text = rows.first().and_then(|r| r["payload"].as_str()).ok_or(())?;
        if text.len() > 1_000_000 {
            return Err(());
        }
        bytes.extend(STANDARD.decode(text).map_err(|_| ())?);
    }
    let saved: Saved<T> = decode(&bytes, Utc::now())?;
    if saved.built_at != built {
        return Err(());
    }
    Ok(Some(saved))
}
pub(super) async fn restore<T: DeserializeOwned + Send + 'static>(
    audit: Option<AuditDb>,
    kind: Kind,
) -> Option<Saved<T>> {
    let audit = audit?;
    match tokio::task::spawn_blocking(move || load(&audit, kind)).await {
        Ok(Ok(saved)) => saved,
        _ => {
            tracing::warn!(
                cache = kind.table(),
                "job-copy report snapshot restore failed"
            );
            None
        }
    }
}
pub(super) async fn persist<T: Serialize + Send + Sync + 'static>(
    audit: Option<AuditDb>,
    kind: Kind,
    data: Arc<T>,
    built_at: DateTime<Utc>,
) {
    let Some(audit) = audit else {
        return;
    };
    if !matches!(
        tokio::task::spawn_blocking(move || save(&audit, kind, &*data, built_at)).await,
        Ok(Ok(()))
    ) {
        tracing::warn!(cache = kind.table(), "job-copy report snapshot save failed");
    }
}
pub(super) fn instant(built_at: DateTime<Utc>, ttl: Duration) -> std::time::Instant {
    let age = Utc::now()
        .signed_duration_since(built_at)
        .to_std()
        .unwrap_or(ttl);
    let now = std::time::Instant::now();
    now.checked_sub(age).unwrap_or(now - ttl)
}
pub(super) async fn ensure_restored(service: &JobReadService) {
    service
        .reports_restored
        .get_or_init(|| async {
            // A failure in one cache does not prevent the other from restoring.
            tokio::join!(
                recent_changes::restore(service),
                owner_todo::restore_facts(service)
            );
        })
        .await;
}

#[cfg(test)]
#[path = "report_snapshot_tests.rs"]
mod tests;

#[cfg(test)]
pub(super) fn sizes<T: Serialize>(data: &T) -> (usize, usize, usize, usize) {
    let (bytes, json) = encode(data, Utc::now()).unwrap();
    let parts = bytes.len().div_ceil(CHUNK_BYTES);
    let base64 = bytes
        .chunks(CHUNK_BYTES)
        .map(|part| STANDARD.encode(part).len())
        .sum();
    (json, bytes.len(), parts, base64)
}
