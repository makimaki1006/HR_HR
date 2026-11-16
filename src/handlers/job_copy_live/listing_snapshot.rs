//! Versioned, disposable index cache in the existing audit store. Never log payloads/errors.
use super::*;
use crate::audit::AuditDb;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::io::{Read, Write};

// v4: リクロジ（公開前を含む）を索引に追加。欠落する旧索引は復元しない。
const VERSION: i64 = 4;
// Base64 text stays at 1,000,000 bytes, leaving room for the pipeline envelope.
const CHUNK_BYTES: usize = 750_000;
const MAX_PARTS: usize = 128;
const MAX_JSON_BYTES: u64 = 256 * 1024 * 1024;

pub(super) fn media<'de, D: serde::Deserializer<'de>>(d: D) -> Result<&'static str, D::Error> {
    match String::deserialize(d)?.as_str() {
        "hrh" => Ok("hrh"),
        "airwork" => Ok("airwork"),
        "reclogi" => Ok("reclogi"),
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
    #[serde(default)]
    assignment: owner_todo::Assignment,
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
/// 最後に保存した (または復元した) 控え。中身が同じなら作り直しても保存し直さないために持つ。
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Saved {
    generation: String,
    parts: i64,
    digest: [u8; 32],
}
/// 控えの JSON の要約値。索引の作成時刻 (`built_at`) の値は除くので、
/// HubSpot から作り直して中身が同じなら同じ値になる。
fn digest(json: &[u8], built_at: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let pattern = format!(
        "\"built_at\":{}",
        serde_json::to_string(built_at).unwrap_or_default()
    );
    let pattern = pattern.as_bytes();
    let mut hasher = Sha256::new();
    let mut rest = json;
    while let Some(at) = rest.windows(pattern.len()).position(|w| w == pattern) {
        hasher.update(&rest[..at]);
        hasher.update(b"\"built_at\":null");
        rest = &rest[at + pattern.len()..];
    }
    hasher.update(rest);
    hasher.finalize().into()
}
fn to_json(index: &Index) -> Result<Vec<u8>, ()> {
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
                assignment: d.assignment.clone(),
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
    Ok(json)
}
fn gzip(json: &[u8]) -> Result<Vec<u8>, ()> {
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(json).map_err(|_| ())?;
    gzip.finish().map_err(|_| ())
}
#[cfg(test)]
fn encode(index: &Index) -> Result<(Vec<u8>, usize), ()> {
    let json = to_json(index)?;
    Ok((gzip(&json)?, json.len()))
}
/// `confirmed_at` は、この中身が HubSpot と同じだと最後に確かめた時刻 (行の `built_at`)。
/// 中身が変わらない間は保存し直さず行の時刻だけを進めるので、中身の作成時刻より新しいことがある。
/// 復元した索引の `built_at` はこの時刻にする。
fn decode(
    bytes: &[u8],
    now: chrono::DateTime<chrono::Utc>,
    confirmed_at: Option<&str>,
) -> Result<(Index, [u8; 32]), ()> {
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
    let confirmed_text = confirmed_at.unwrap_or(&saved.index.built_at).to_owned();
    let confirmed = chrono::DateTime::parse_from_rfc3339(&confirmed_text).map_err(|_| ())?;
    let age = now.signed_duration_since(confirmed);
    if saved.version != VERSION
        || confirmed < built
        || age < chrono::Duration::zero()
        || age > chrono::Duration::hours(24)
        || saved.listings.len() != saved.index.records.len()
        || saved.deals.len() != saved.index.deals.len()
    {
        return Err(());
    }
    let digest = digest(&json, &saved.index.built_at);
    drop(json);
    for (row, private) in saved.index.records.iter_mut().zip(saved.listings) {
        row.indexed_application_count = private.applications;
        row.body_summary = private.body_summary;
        row.perf = private.perf;
    }
    for (deal, private) in saved.index.deals.iter_mut().zip(saved.deals) {
        deal.assignment = private.assignment;
        deal.pipeline = private.pipeline;
        deal.stage = private.stage;
        deal.created_at = private.created_at;
    }
    saved.index.built_at = confirmed_text;
    Ok((saved.index, digest))
}
/// 中身が前回の控え (`previous`) と同じなら、控えを送り直さず、その行の時刻だけを進める
/// (UPDATE 1 文)。違う・前回が無い・時刻を進められなかったときは全部を保存する。
fn save(audit: &AuditDb, index: &Index, previous: Option<&Saved>) -> Result<Saved, ()> {
    let json = to_json(index)?;
    let json_bytes = json.len();
    let digest = digest(&json, &index.built_at);
    if let Some(previous) = previous.filter(|p| p.digest == digest) {
        let rows = audit.turso().query(
            "UPDATE job_copy_index_snapshots SET built_at = ? WHERE generation = ? AND version = ? RETURNING part",
            &[&index.built_at, &previous.generation, &VERSION],
        );
        if matches!(&rows, Ok(rows) if rows.len() as i64 == previous.parts) {
            tracing::info!(
                json_bytes,
                parts = previous.parts,
                "job-copy index snapshot unchanged, kept the saved copy"
            );
            return Ok(previous.clone());
        }
    }
    let bytes = gzip(&json)?;
    drop(json);
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
    Ok(Saved {
        generation,
        parts: total as i64,
        digest,
    })
}
fn load(audit: &AuditDb) -> Result<Option<(Index, Saved)>, ()> {
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
    let built_text = row["built_at"].as_str().ok_or(())?.to_owned();
    let built = chrono::DateTime::parse_from_rfc3339(&built_text).map_err(|_| ())?;
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
    let (index, digest) = decode(&bytes, chrono::Utc::now(), Some(&built_text))?;
    Ok(Some((
        index,
        Saved {
            generation,
            parts: total,
            digest,
        },
    )))
}
pub(super) async fn restore(cache: &Cache, audit: Option<AuditDb>) -> Option<Saved> {
    let audit = audit?;
    // メモリ計測: 保存した求人一覧の復元 (gzip を伸ばして JSON を読む)
    let _task = crate::mem_watch::task("job_copy.index_restore");
    match tokio::task::spawn_blocking(move || load(&audit)).await {
        Ok(Ok(Some((index, saved)))) => {
            let mut state = cache.state.write().await;
            state.snapshot = Some(Arc::new(index));
            state.refreshing = true;
            Some(saved)
        }
        Ok(Ok(None)) => None,
        _ => {
            tracing::warn!("job-copy index snapshot restore failed");
            None
        }
    }
}
/// `previous` は前回保存した (または復元した) 控え。保存できたら新しい控えに置き換える。
pub(super) async fn persist(
    index: Arc<Index>,
    audit: Option<AuditDb>,
    previous: &mut Option<Saved>,
) {
    let Some(audit) = audit else {
        return;
    };
    let last = previous.clone();
    // メモリ計測: 求人一覧の保存 (JSON にして gzip・Base64 にし、Turso へ分けて書く)
    let _task = crate::mem_watch::task("job_copy.index_persist");
    match tokio::task::spawn_blocking(move || save(&audit, &index, last.as_ref())).await {
        Ok(Ok(saved)) => *previous = Some(saved),
        _ => tracing::warn!("job-copy index snapshot save failed"),
    }
}

#[cfg(test)]
#[path = "listing_snapshot_tests.rs"]
mod tests;

#[cfg(test)]
pub(super) async fn test_round_trip(index: Index) -> Index {
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    tokio::task::spawn_blocking(move || {
        save(&audit, &index, None).unwrap();
        load(&audit).unwrap().unwrap().0
    })
    .await
    .unwrap()
}
