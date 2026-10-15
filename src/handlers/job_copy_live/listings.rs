//! Complete in-memory index: deal associations and structured comparison summaries.
//! Bodies/history are transient; applicant properties are never read or stored.
use super::listing_retry::IndexReader;
use super::*;
use axum::extract::Path as AxumPath;
use unicode_normalization::UnicodeNormalization;
#[path = "listing_deals.rs"]
mod deals;
use deals::{deal_candidates, index_associations, publication, summary, Deal, Summary};
pub(super) use deals::{read_body_dates, read_comparison};
#[path = "listing_pipelines.rs"]
mod pipelines;
pub(super) use pipelines::company_deals;
#[path = "listing_home.rs"]
mod home;
pub(super) use home::read as read_home;
#[cfg(test)]
#[path = "listing_cache_tests.rs"]
mod cache_tests;
#[cfg(test)]
#[path = "listing_deal_tests.rs"]
mod deal_tests;

/// AirWork image URLs, one per line. Written by `scripts/job_copy_airwork_image_urls.py`.
pub(super) const AW_IMAGES: &str = "aw_kyuujinhyou_gazou";
/// Weekly Indeed (HRハッカー) results as JSON. Written by `scripts/indeed_weekly_import.py`.
pub(super) const INDEED_WEEKLY_PERF: &str = "indeed_weekly_perf";
/// Weekly AirWork results as JSON, same shape with `"source":"airwork"`. Filled by a separate sync.
pub(super) const AIRWORK_WEEKLY_PERF: &str = "airwork_weekly_perf";
const INDEED_PERF_MAX_BYTES: usize = 200_000;
/// The stored JSON as a value, or None when it is empty, too large or not the expected shape.
/// The screen checks every field again; a bad value must never break the page.
pub(super) fn indeed_weekly_perf(record: &Record) -> Option<Value> {
    weekly_perf(record, INDEED_WEEKLY_PERF, "hrhacker", None)
}
/// AirWork results; `job_id` must be a non-empty digit string equal to the listing's AirWork id.
pub(super) fn airwork_weekly_perf(record: &Record) -> Option<Value> {
    let own = record.value("id_airwork")?.trim();
    if own.is_empty() || !own.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    weekly_perf(record, AIRWORK_WEEKLY_PERF, "airwork", Some(own))
}
/// Media-agnostic reader of a weekly-results property.
fn weekly_perf(
    record: &Record,
    property: &str,
    source: &str,
    job_id: Option<&str>,
) -> Option<Value> {
    let raw = record.value(property)?;
    if raw.len() > INDEED_PERF_MAX_BYTES {
        return None;
    }
    let value: Value = serde_json::from_str(raw).ok()?;
    (value.get("schema_version").and_then(Value::as_u64) == Some(1)
        && value.get("source").and_then(Value::as_str) == Some(source)
        && job_id.is_none_or(|id| value.get("job_id").and_then(Value::as_str) == Some(id))
        && value.get("rows").is_some_and(Value::is_array))
    .then_some(value)
}
/// Marker line meaning "the public page was checked and has no photos".
pub(super) const AW_NO_IMAGES: &str = "NO_IMAGES";
const AW_IMAGE_LIMIT: usize = 50;
const PROPERTIES: &[&str] = &[
    "jobgen_draft_status",
    "hs_name",
    "id_hrhakkaa",
    "id_airwork",
    "airwork_account_login_id",
    "id_shop_hrhakkaa",
    "hrh_kyuujinhyou_honbun",
    "hrh_kyuujinhyou_gazou",
    AW_IMAGES,
    "todoufuken",
    "shikuchouson",
    "qinwude",
    "zhizhong",
    "shigotonaiyou",
    "kyuuyokeitai",
    "kinmujikan",
    "kyuujitsu",
    "guyongxingtai",
    "fukurikousei",
    "kyuujin_status",
    "baitai_genjoukyou_hrhakkaa",
    "baitai_genjoukyou_airwork",
    "saishuu_csv_kenshutsu_bi",
    "koukai_kaishi_nichiji",
    INDEED_TITLE_PROPERTY,
    INDEED_WEEKLY_PERF,
    AIRWORK_WEEKLY_PERF,
];
#[derive(Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListingsQuery {
    prefecture: Option<String>,
    title: Option<String>,
    media: Option<String>,
    #[serde(default)]
    offset: usize,
    sort: Option<String>,
    deal: Option<String>,
    deal_search: Option<String>,
    publication: Option<String>,
    descending: Option<bool>,
    draft_status: Option<String>,
    /// 職種の指定状態で絞る: unset（未指定）/ assigned（指定済み＝職種を指定、または市場と比べない）。
    indeed_state: Option<String>,
    /// 求人名に含まれる文字で絞る（空白・記号・大小を無視）。
    q: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
struct Listing {
    id: String,
    media: &'static str,
    media_job_id: String,
    account_id: Option<String>,
    title: Option<String>,
    prefecture: Option<String>,
    municipality: Option<String>,
    /// 人が指定した Indeed 職種（指定済みのときだけ）。一覧の「職種」絞り込みはこれを使う。
    category: Option<String>,
    indeed_title: Option<String>,
    /// "set"（職種を指定）/ "none"（市場と比べない）/ "unset"（未指定）
    indeed_title_state: &'static str,
    publication_status: Option<String>,
    draft_status: Option<String>,
    last_csv_detected_at: Option<String>,
    deal_ids: Vec<String>,
    summary: Summary,
    #[serde(skip)]
    indexed_application_count: Option<usize>,
    /// 週ごとの実績の要約（ホーム用）。週ごとの JSON そのものは索引に残さない。
    #[serde(skip)]
    perf: Option<home::PerfDigest>,
}
fn normalize(value: &str) -> String {
    value
        .nfkc()
        .filter(|c| !c.is_whitespace() && !"・／/()（）【】".contains(*c))
        .flat_map(char::to_lowercase)
        .collect()
}
/// 「市場と比べない」を表す保存値。Indeed の職種名とは重ならない。
pub(super) const INDEED_TITLE_NONE: &str = "__none__";
/// HubSpot の求人の項目。人が指定した Indeed 職種（または `__none__`）を持つ。
pub(super) use crate::hubspot::client::INDEED_TITLE_PROPERTY;
/// 指定の状態。職種は人が指定したものだけを使い、文面からは推測しない。
fn indeed_title_of(record: &Record) -> (Option<String>, &'static str) {
    match record.value(INDEED_TITLE_PROPERTY).map(str::trim) {
        None | Some("") => (None, "unset"),
        Some(INDEED_TITLE_NONE) => (None, "none"),
        Some(v) => (Some(v.to_owned()), "set"),
    }
}
fn location(value: &str) -> (Option<String>, Option<String>) {
    let text: String = value.nfkc().collect();
    let text = text.trim();
    let pref = crate::models::job_seeker::PREFECTURE_ORDER
        .iter()
        .find(|pref| text.starts_with(**pref));
    match pref {
        Some(pref) => {
            let rounded = applicant_area::round_area(Some(pref), Some(text));
            (Some((*pref).to_owned()), rounded.municipality)
        }
        None => (None, None),
    }
}
fn listing(record: &Record) -> Option<Listing> {
    let hrh = record.value("id_hrhakkaa");
    let aw = record.value("id_airwork");
    let (media, media_job_id, account_id) = match (hrh, aw) {
        (Some(id), None) => ("hrh", id, record.value("id_shop_hrhakkaa")),
        (None, Some(id)) => ("airwork", id, record.value("airwork_account_login_id")),
        _ => return None,
    };
    let (prefecture, municipality) = if media == "hrh" {
        (
            record.value("todoufuken").map(str::to_owned),
            record.value("shikuchouson").map(str::to_owned),
        )
    } else {
        location(record.value("qinwude").unwrap_or(""))
    };
    let (indeed_title, indeed_title_state) = indeed_title_of(record);
    Some(Listing {
        id: record.id.clone(),
        media,
        media_job_id: media_job_id.into(),
        account_id: account_id.map(str::to_owned),
        title: record.value("hs_name").map(str::to_owned),
        prefecture,
        municipality,
        category: indeed_title.clone(),
        indeed_title,
        indeed_title_state,
        publication_status: publication(record, media),
        deal_ids: vec![],
        summary: summary(record, media),
        indexed_application_count: None,
        perf: home::PerfDigest::from_record(record, media),
        draft_status: record
            .value("jobgen_draft_status")
            .filter(|v| ["pending", "adopted", "rejected"].contains(v))
            .map(str::to_owned),
        last_csv_detected_at: record.value("saishuu_csv_kenshutsu_bi").map(str::to_owned),
    })
}
fn taxonomy(db: &crate::db::local_sqlite::LocalDb) -> Result<Vec<String>, ReadError> {
    db.query(
        "SELECT DISTINCT norm_title FROM insight_title_pref ORDER BY norm_title",
        &[],
    )
    .map(|rows| {
        rows.iter()
            .map(|row| super::super::helpers::get_str(row, "norm_title"))
            .collect()
    })
    .map_err(|_| fail("market_data_unavailable"))
}
pub(super) fn service(access: &Access) -> Result<&JobReadService, ReadError> {
    access.service.as_deref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))
}
const INDEX_TTL: Duration = Duration::from_secs(6 * 60 * 60);
// No fixed pause between completed pages; starts are spaced across the whole build.
const PAGE_PAUSE: Duration = Duration::ZERO;
// Initial display estimate from the October 2026 production count; never used for indexing.
const INITIAL_ESTIMATED_TOTAL: usize = 41_000;
const INDEX_RANGE_COUNT: usize = 8;
fn retry_delay(failures: usize) -> Duration {
    Duration::from_secs(match failures {
        0 | 1 => 60,
        2 => 120,
        3 => 300,
        _ => 600,
    })
}
const PAGE_SIZE: usize = 50;

#[derive(Default)]
pub(super) struct IndexCache {
    state: tokio::sync::RwLock<IndexState>,
    worker: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Home company refresh and encoded response caches (process-local only).
    home: home::Cache,
    /// 取引のパイプライン・ステージの名前（6 時間使い回す）。
    pipeline_defs: tokio::sync::Mutex<Option<pipelines::CachedDefs>>,
}
impl Drop for IndexCache {
    fn drop(&mut self) {
        if let Ok(worker) = self.worker.get_mut() {
            if let Some(worker) = worker.take() {
                worker.abort();
            }
        }
    }
}
pub(super) type Cache = Arc<IndexCache>;
#[derive(Default)]
struct IndexState {
    snapshot: Option<Arc<Index>>,
    refreshing: bool,
    refresh_failed: bool,
    draft_updates: BTreeMap<String, String>,
    read_count: usize,
    estimated_total: Option<usize>,
    consecutive_failures: usize,
    retry_at: Option<tokio::time::Instant>,
}
#[derive(Serialize)]
struct PreparationProgress {
    read_count: usize,
    estimated_total: usize,
    retry_after_seconds: Option<u64>,
}
impl IndexState {
    fn progress(&self) -> PreparationProgress {
        PreparationProgress {
            read_count: self.read_count,
            estimated_total: self
                .estimated_total
                .unwrap_or(INITIAL_ESTIMATED_TOTAL)
                .max(self.read_count),
            retry_after_seconds: self.retry_at.map(|at| {
                at.saturating_duration_since(tokio::time::Instant::now())
                    .as_secs()
                    .max(1)
            }),
        }
    }
    fn failed(&mut self) {
        self.refreshing = false;
        self.refresh_failed = true;
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.retry_at = Some(tokio::time::Instant::now() + retry_delay(self.consecutive_failures));
    }
}
#[derive(Clone)]
struct Index {
    records: Vec<Listing>,
    titles: Vec<String>,
    built_at: String,
    deals: Vec<Deal>,
}
#[derive(Serialize)]
struct PageListing {
    #[serde(flatten)]
    listing: Listing,
    application_count: Option<usize>,
}

/// 職種の指定を書けたら、全件索引の行にも反映する（再構築を待たない）。
/// `value` は書いた保存値（職種名、または `__none__`）。
pub(super) async fn update_indeed_title(service: &JobReadService, id: &str, value: &str) {
    let mut cache = service.listing_index.state.write().await;
    if let Some(index) = cache.snapshot.as_mut() {
        let index = Arc::make_mut(index);
        if let Some(row) = index.records.iter_mut().find(|r| r.id == id) {
            if value == INDEED_TITLE_NONE {
                row.indeed_title = None;
                row.indeed_title_state = "none";
            } else {
                row.indeed_title = Some(value.to_owned());
                row.indeed_title_state = "set";
            }
            row.category = row.indeed_title.clone();
        }
    }
}
/// 市場データにある職種名の一覧（指定できる職種）。
pub(super) fn market_titles(
    db: &crate::db::local_sqlite::LocalDb,
) -> Result<Vec<String>, ReadError> {
    taxonomy(db)
}

// 保存に成功した案だけ一覧へ反映する。全件索引の絞り込み・件数にも使用する。
pub(super) async fn update_draft_state(
    service: &JobReadService,
    id: &str,
    status: crate::job_gen::drafts::DraftStatus,
) {
    let mut cache = service.listing_index.state.write().await;
    let status = serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned));
    if cache.refreshing {
        if let Some(value) = &status {
            cache.draft_updates.insert(id.to_owned(), value.clone());
        }
    }
    if let Some(index) = cache.snapshot.as_mut() {
        let index = Arc::make_mut(index);
        if let Some(row) = index.records.iter_mut().find(|r| r.id == id) {
            row.draft_status = status;
        }
    }
}

/// Half-open ranges [lower, upper); the first/last ranges cover both open tails.
#[derive(Clone, Copy, Debug)]
struct IdRange {
    lower: u128,
    upper: Option<u128>,
}
fn numeric_id(id: &str) -> Result<u128, ReadError> {
    valid_id(id).map_err(|_| fail("hubspot_invalid_response"))?;
    id.parse().map_err(|_| fail("hubspot_invalid_response"))
}
fn ranges_from_cuts(mut cuts: Vec<u128>) -> Vec<IdRange> {
    cuts.push(0);
    cuts.sort_unstable();
    cuts.dedup();
    cuts.iter()
        .enumerate()
        .map(|(i, &lower)| IdRange {
            lower,
            upper: cuts.get(i + 1).copied(),
        })
        .collect()
}
async fn index_ranges(
    cache: &Cache,
    reader: &IndexReader,
    count: usize,
) -> Result<Vec<IdRange>, ReadError> {
    let previous = cache.state.read().await.snapshot.clone();
    if let Some(index) = previous {
        let mut ids = index
            .records
            .iter()
            .map(|r| numeric_id(&r.id))
            .collect::<Result<Vec<_>, _>>()?;
        ids.sort_unstable();
        ids.dedup();
        if !ids.is_empty() {
            return Ok(ranges_from_cuts(
                (1..count).map(|i| ids[ids.len() * i / count]).collect(),
            ));
        }
    }
    // Lightweight first page establishes the minimum. Small/empty accounts need no Search.
    let data = reader
        .read(
            "/crm/v3/objects/0-420",
            &[
                ("limit", "100".into()),
                ("properties", "hs_object_id".into()),
            ],
            None,
            &super::listing_retry::DELAYS[..1],
        )
        .await?;
    let page: Page = serde_json::from_value(data).map_err(|_| fail("hubspot_invalid_response"))?;
    if page.paging.and_then(|p| p.next).is_none() {
        return Ok(ranges_from_cuts(vec![]));
    }
    let min = page
        .results
        .first()
        .ok_or_else(|| fail("hubspot_invalid_response"))?;
    let min = numeric_id(&min.id)?;
    // Search is used only for the maximum ID, never for paging beyond its 10k cap.
    let body = json!({
        "limit": 1,
        "properties": ["hs_object_id"],
        "sorts": [{"propertyName": "hs_object_id", "direction": "DESCENDING"}]
    });
    let data = reader
        .read(
            "/crm/v3/objects/0-420/search",
            &[],
            Some(&body),
            &super::listing_retry::DELAYS[..1],
        )
        .await?;
    let page: Page = serde_json::from_value(data).map_err(|_| fail("hubspot_invalid_response"))?;
    let max = numeric_id(
        &page
            .results
            .first()
            .ok_or_else(|| fail("hubspot_invalid_response"))?
            .id,
    )?;
    if max < min {
        return Err(fail("hubspot_invalid_response"));
    }
    Ok(ranges_from_cuts(
        (1..count)
            .map(|i| min + (max - min + 1) * i as u128 / count as u128)
            .collect(),
    ))
}
async fn read_index_range(
    cache: &Cache,
    reader: &IndexReader,
    range: IdRange,
    number: usize,
    pause: Duration,
) -> Result<(BTreeSet<String>, Vec<Listing>), ReadError> {
    let started = std::time::Instant::now();
    let mut ids = BTreeSet::new();
    let mut rows = Vec::new();
    // Verified against the real HubSpot 0-420 list API on 2026-10-15:
    // after=X includes X (IDs >= X), so [lower, upper) starts at after=lower.
    let mut after = range.lower;
    let properties = index_properties().join(",");
    loop {
        let mut query = vec![("limit", "100".into()), ("properties", properties.clone())];
        query.push(("after", after.to_string()));
        // Retry only this failed page once; completed pages/ranges are never restarted.
        let data = reader
            .read(
                "/crm/v3/objects/0-420",
                &query,
                None,
                &super::listing_retry::DELAYS[..1],
            )
            .await?;
        let page: Page =
            serde_json::from_value(data.clone()).map_err(|_| fail("hubspot_invalid_response"))?;
        if page.results.len() > 100 {
            return Err(fail("hubspot_invalid_response"));
        }
        let mut crossed = false;
        let before = ids.len();
        let mut last = None;
        for record in page.results {
            let id = numeric_id(&record.id)?;
            if id < after || last.is_some_and(|last| id < last) {
                return Err(fail("listing_paging_invalid"));
            }
            last = Some(id);
            if range.upper.is_some_and(|upper| id >= upper) {
                crossed = true;
                continue;
            }
            if id < range.lower {
                return Err(fail("listing_paging_invalid"));
            }
            if ids.insert(record.id.clone()) {
                if let Some(row) = listing(&record) {
                    rows.push(row);
                }
            }
        }
        {
            let mut state = cache.state.write().await;
            state.read_count += ids.len() - before;
            if let Some(total) = data["total"].as_u64().and_then(|n| usize::try_from(n).ok()) {
                state.estimated_total = Some(total);
            }
        }
        if crossed {
            break;
        }
        let Some(next) = page.paging.and_then(|p| p.next) else {
            break;
        };
        let next = numeric_id(&next.after)?;
        // Verified against the real HubSpot 0-420 list API on 2026-10-15:
        // next.after is the last ID + 1, not the last ID. Only require forward
        // progress beyond the last record; a numeric cursor can fall in an ID gap.
        if next <= after || last.is_none_or(|last| next <= last) {
            return Err(fail("listing_paging_invalid"));
        }
        after = next;
        if !pause.is_zero() {
            tokio::time::sleep(pause).await;
        }
    }
    tracing::info!(
        range_number = number,
        count = ids.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "job-copy index range"
    );
    Ok((ids, rows))
}

/// One complete build. Only non-ended listings fetch bodies for structured summaries.
async fn build_index(
    cache: &Cache,
    service: &JobReadService,
    titles: Vec<String>,
    pause: Duration,
    range_count: usize,
) -> Result<Index, ReadError> {
    let started = std::time::Instant::now();
    let reader = IndexReader::new(&service.hs.background());
    let ranges = if range_count == 1 {
        ranges_from_cuts(vec![])
    } else {
        index_ranges(cache, &reader, range_count).await?
    };
    tracing::info!(range_count = ranges.len(), "job-copy index ranges");
    let mut tasks = tokio::task::JoinSet::new();
    for (number, range) in ranges.into_iter().enumerate() {
        let (cache, reader) = (cache.clone(), reader.clone());
        tasks.spawn(async move { read_index_range(&cache, &reader, range, number, pause).await });
    }
    let mut ids = BTreeSet::new();
    let mut records = Vec::new();
    let mut counted = 0;
    let mut mapping = BTreeMap::new();
    while let Some(result) = tasks.join_next().await {
        let (range_ids, mut rows) = result.map_err(|_| fail("hubspot_read_failed"))??;
        enrich_index_summaries(&reader, &mut rows).await?;
        counted += range_ids.len();
        // Enrich one completed range while the other ranges keep reading pages.
        // Only one enrichment runs at a time, retaining the four-batch bound.
        let listing_ids: Vec<_> = rows.iter().map(|r| r.id.clone()).collect();
        let stage = std::time::Instant::now();
        mapping.extend(index_associations(&reader, &listing_ids, "deals").await?);
        for group in rows.chunks_mut(4000) {
            let ids: Vec<_> = group.iter().map(|r| r.id.clone()).collect();
            let counts = index_associations(&reader, &ids, "0-421").await?;
            for row in group {
                row.indexed_application_count = counts.get(&row.id).map(Vec::len);
            }
        }
        tracing::info!(
            count = rows.len(),
            elapsed_ms = stage.elapsed().as_millis() as u64,
            "job-copy index range associations"
        );
        for row in rows {
            if !ids.contains(&row.id) {
                records.push(row);
            }
        }
        ids.extend(range_ids);
    }
    // Every ID has exactly one owning range. Deduplicate defensively and reconcile
    // raw IDs (including records without a supported media) with aggregate progress.
    let mut state = cache.state.write().await;
    if counted != ids.len() || state.read_count != ids.len() {
        return Err(fail("hubspot_partial_read"));
    }
    state.read_count = ids.len();
    let expected_total = state.estimated_total;
    drop(state);
    tracing::info!(
        expected_total,
        count = ids.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "job-copy index pages and associations"
    );
    let stage = std::time::Instant::now();
    let deal_ids: Vec<_> = mapping
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut names = BTreeMap::new();
    // 納品管理のパイプラインの取引だけを扱う。ほかの取引は無いものとして数える。
    for record in deals::index_deal_records(&reader, &deal_ids).await? {
        if pipelines::is_delivery(record.value("pipeline")) {
            names.insert(
                record.id.clone(),
                (
                    record.value("dealname").map(str::to_owned),
                    record.value("pipeline").map(str::to_owned),
                    record.value("dealstage").map(str::to_owned),
                    record
                        .value("createdate")
                        .map(|v| v.chars().take(10).collect()),
                ),
            );
        }
    }
    tracing::info!(
        count = deal_ids.len(),
        elapsed_ms = stage.elapsed().as_millis() as u64,
        "job-copy index deal names"
    );
    let mut deal_counts = BTreeMap::<String, usize>::new();
    for row in &mut records {
        row.deal_ids = mapping
            .get(&row.id)
            .ok_or_else(|| fail("hubspot_partial_read"))?
            .iter()
            .filter(|id| names.contains_key(*id))
            .cloned()
            .collect();
        for id in &row.deal_ids {
            *deal_counts.entry(id.clone()).or_default() += 1;
        }
    }
    let deals = deal_counts
        .into_iter()
        .map(|(id, count)| {
            let (name, pipeline, stage, created_at) = names.remove(&id).unwrap_or_default();
            Deal {
                name,
                pipeline,
                stage,
                created_at,
                id,
                count,
            }
        })
        .collect();
    records.shrink_to_fit();
    Ok(Index {
        records,
        titles,
        built_at: chrono::Utc::now().to_rfc3339(),
        deals,
    })
}
pub(super) async fn refresh(
    cache: &Cache,
    service: &JobReadService,
    titles: Vec<String>,
    pause: Duration,
) -> Result<(), ReadError> {
    let started = std::time::Instant::now();
    {
        let mut state = cache.state.write().await;
        state.refreshing = true;
        state.refresh_failed = false;
        state.retry_at = None;
        state.read_count = 0;
    }
    let result = build_index(cache, service, titles, pause, INDEX_RANGE_COUNT).await;
    let mut state = cache.state.write().await;
    state.refreshing = false;
    state.refresh_failed = result.is_err();
    match result {
        Ok(mut index) => {
            for row in &mut index.records {
                if let Some(status) = state.draft_updates.get(&row.id) {
                    row.draft_status = Some(status.clone());
                }
            }
            state.draft_updates.clear();
            let index = Arc::new(index);
            tracing::info!(
                count = index.records.len(),
                elapsed_ms = started.elapsed().as_millis() as u64,
                "job-copy index build"
            );
            state.snapshot = Some(index);
            state.consecutive_failures = 0;
            state.estimated_total = Some(state.read_count);
            Ok(())
        }
        Err(error) => {
            tracing::warn!(
                count = state.read_count,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "job-copy index build failed"
            );
            state.failed();
            Err(error) // An incomplete build never removes the previous snapshot.
        }
    }
}
/// Runs once at startup and every six hours, even when no one opens the screen.
/// The worker holds only a Weak cache so dropping the router/service cancels it.
pub(super) fn start_worker(service: &Arc<JobReadService>, db: crate::db::local_sqlite::LocalDb) {
    let cache = &service.listing_index;
    let Ok(mut worker) = cache.worker.lock() else {
        return;
    };
    if worker.is_some() {
        return;
    }
    let weak = Arc::downgrade(cache);
    // Keep the HTTP client, not the cache/service; holding the service here would form a cycle.
    let hs = service.hs.background();
    *worker = Some(tokio::spawn(async move {
        loop {
            let Some(cache) = weak.upgrade() else {
                return;
            };
            let service = JobReadService::from_client(hs.clone());
            let result = match taxonomy(&db) {
                Ok(titles) => refresh(&cache, &service, titles, PAGE_PAUSE).await,
                Err(error) => {
                    cache.state.write().await.failed();
                    Err(error)
                }
            };
            if let Err(error) = &result {
                tracing::warn!(code = error.code(), "job-copy listing index refresh failed");
            }
            if result.is_ok() {
                let snapshot = cache.state.read().await.snapshot.clone();
                if let Some(index) = snapshot {
                    // Attach the shared cache: the worker's temporary service has its own caches.
                    let mut home_service = service.clone();
                    home_service.listing_index = cache.clone();
                    home::prefetch(&home_service, &index).await;
                }
            }
            let delay = if result.is_ok() {
                INDEX_TTL
            } else {
                cache
                    .state
                    .read()
                    .await
                    .retry_at
                    .map(|at| at.saturating_duration_since(tokio::time::Instant::now()))
                    .unwrap_or_else(|| retry_delay(1))
            };
            drop(cache);
            tokio::time::sleep(delay).await;
        }
    }));
}
fn indeed_state_matches(row: &Listing, state: Option<&str>) -> bool {
    match state {
        Some("unset") => row.indeed_title_state == "unset",
        Some("assigned") => row.indeed_title_state != "unset",
        _ => true,
    }
}
fn matches(row: &Listing, query: &ListingsQuery) -> bool {
    matches_except_state(row, query) && indeed_state_matches(row, query.indeed_state.as_deref())
}
fn matches_except_state(row: &Listing, query: &ListingsQuery) -> bool {
    query.q.as_ref().is_none_or(|q| {
        let needle = normalize(q);
        needle.is_empty() || normalize(row.title.as_deref().unwrap_or("")).contains(&needle)
    }) && query.deal.as_ref().is_none_or(|v| {
        if v == "none" {
            row.deal_ids.is_empty()
        } else {
            // 取引 ID はカンマ区切りで複数指定できる（顧客の全部の取引の求人を 1 度に出すため）。
            v.split(',').any(|id| row.deal_ids.iter().any(|d| d == id))
        }
    }) && query.publication.as_ref().is_none_or(|v| {
        if v == "unknown" {
            row.publication_status
                .as_deref()
                .is_none_or(|status| status == "不明")
        } else {
            Some(v) == row.publication_status.as_ref()
        }
    }) && query.draft_status.as_ref().is_none_or(|v| {
        if v == "none" {
            row.draft_status.is_none()
        } else {
            Some(v) == row.draft_status.as_ref()
        }
    }) && query.media.as_ref().is_none_or(|v| v == row.media)
        && query
            .prefecture
            .as_ref()
            .is_none_or(|v| Some(v) == row.prefecture.as_ref())
        && query.title.as_ref().is_none_or(|v| {
            if v == "unknown" {
                row.category.is_none()
            } else {
                Some(v) == row.category.as_ref()
            }
        })
}
/// 公開状況ごとの件数（公開状況以外の絞り込みは反映）。画面の「掲載中／掲載終了／すべて」に使う。
fn publication_counts(records: &[Listing], query: &ListingsQuery) -> Value {
    let mut without_publication = query.clone();
    without_publication.publication = None;
    let mut counts = [0usize; 4];
    for row in records.iter().filter(|r| matches(r, &without_publication)) {
        match row.publication_status.as_deref() {
            Some("掲載中") => counts[0] += 1,
            Some("掲載終了") => counts[1] += 1,
            Some("掲載前") => counts[2] += 1,
            _ => counts[3] += 1,
        }
    }
    json!({"live":counts[0], "ended":counts[1], "before":counts[2], "unknown":counts[3]})
}
fn selected_page(index: &Index, query: &ListingsQuery) -> (usize, Vec<PageListing>) {
    let mut rows: Vec<_> = index
        .records
        .iter()
        .filter(|row| matches(row, query))
        .collect();
    rows.sort_by(|a, b| {
        let id_order = || a.id.len().cmp(&b.id.len()).then_with(|| a.id.cmp(&b.id));
        fn summary_value<'a>(r: &'a Listing, sort: Option<&str>) -> Option<&'a str> {
            match sort {
                Some("occupation") => r.summary.occupation.as_deref(),
                Some("location") => r.summary.location.as_deref(),
                Some("salary_kind") => r.summary.salary_kind.as_deref(),
                Some("salary_min") => r.summary.salary_min.as_deref(),
                Some("salary_max") => r.summary.salary_max.as_deref(),
                Some("hours") => r.summary.hours.as_deref(),
                Some("holidays") => r.summary.holidays.as_deref(),
                Some("body_changed_at") => r.summary.body_changed_at.as_deref(),
                Some("publication") => r.publication_status.as_deref(),
                _ => None,
            }
        }
        let field_order = if query.sort.as_deref() == Some("applications") {
            Some(
                a.indexed_application_count
                    .cmp(&b.indexed_application_count),
            )
        } else if [
            "occupation",
            "location",
            "salary_kind",
            "salary_min",
            "salary_max",
            "hours",
            "holidays",
            "body_changed_at",
            "publication",
        ]
        .contains(&query.sort.as_deref().unwrap_or(""))
        {
            let left = summary_value(a, query.sort.as_deref());
            let right = summary_value(b, query.sort.as_deref());
            let number = |v: Option<&str>| {
                v.and_then(|v| {
                    // 給与表示と同じく全角数字・桁区切り・前後の空白を整える。
                    let normalized: String = v.nfkc().filter(|c| *c != ',').collect();
                    let value = normalized.trim();
                    let mut parts = value.split('.');
                    let digits =
                        |part: &str| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit());
                    if !digits(parts.next()?)
                        || parts.next().is_some_and(|p| !digits(p))
                        || parts.next().is_some()
                    {
                        return None;
                    }
                    value.parse::<f64>().ok().filter(|v| v.is_finite())
                })
            };
            Some(
                if ["salary_min", "salary_max"].contains(&query.sort.as_deref().unwrap_or("")) {
                    number(left)
                        .partial_cmp(&number(right))
                        .unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    left.cmp(&right)
                },
            )
        } else {
            None
        };
        let order = if let Some(order) = field_order {
            order.then_with(id_order)
        } else if query.sort.as_deref() == Some("bulk") {
            // 職種の一括指定: 掲載中を先に、応募の多い順（同数は番号順）。
            let not_live = |r: &Listing| r.publication_status.as_deref() != Some("掲載中");
            not_live(a)
                .cmp(&not_live(b))
                .then_with(|| {
                    b.indexed_application_count
                        .cmp(&a.indexed_application_count)
                })
                .then_with(id_order)
        } else if query.sort.as_deref() == Some("id") {
            id_order()
        } else if query.sort.as_deref() == Some("media") {
            a.media
                .cmp(b.media)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(id_order)
        } else {
            a.title.cmp(&b.title).then_with(id_order)
        };
        if query.descending == Some(true) {
            order.reverse()
        } else {
            order
        }
    });
    let total = rows.len();
    let page = rows
        .into_iter()
        .skip(query.offset)
        .take(PAGE_SIZE)
        .map(|row| PageListing {
            listing: row.clone(),
            application_count: row.indexed_application_count,
        })
        .collect();
    (total, page)
}
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<ListingsQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = service(&access)?;
    let cached = service.listing_index.state.read().await;
    let snapshot = cached.snapshot.clone();
    let refreshing = cached.refreshing;
    let refresh_failed = cached.refresh_failed;
    let progress = cached.progress();
    drop(cached);
    let Some(index) = snapshot else {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(json!({
                "status":"preparing", "listings":[], "total":null, "index_built_at":null,
                "offset":query.offset, "next_offset":null, "titles":[], "refreshing":refreshing,
                "refresh_failed":refresh_failed, "deals":[], "progress":progress
            })),
        ));
    };
    if query
        .draft_status
        .as_deref()
        .is_some_and(|v| !["pending", "adopted", "rejected", "none"].contains(&v))
        || query.publication.as_ref().is_some_and(|v| v.len() > 100)
        || query.q.as_ref().is_some_and(|v| v.chars().count() > 100)
        || query
            .indeed_state
            .as_deref()
            .is_some_and(|v| !["unset", "assigned"].contains(&v))
        || query
            .media
            .as_deref()
            .is_some_and(|v| !["hrh", "airwork"].contains(&v))
        || query
            .title
            .as_ref()
            .is_some_and(|v| v != "unknown" && !index.titles.contains(v))
        || query
            .prefecture
            .as_ref()
            .is_some_and(|v| !crate::models::job_seeker::PREFECTURE_ORDER.contains(&v.as_str()))
        || query.sort.as_deref().is_some_and(|v| {
            ![
                "title",
                "id",
                "media",
                "occupation",
                "location",
                "salary_kind",
                "salary_min",
                "salary_max",
                "hours",
                "holidays",
                "applications",
                "body_changed_at",
                "publication",
                "bulk",
            ]
            .contains(&v)
        })
    {
        return Err(ReadError(StatusCode::BAD_REQUEST, "listing_filter_invalid"));
    }
    if query.deal.as_ref().is_some_and(|id| {
        id != "none"
            && (id.split(',').count() > 20
                || id
                    .split(',')
                    .any(|one| !index.deals.iter().any(|d| d.id == one)))
    }) {
        return Err(ReadError(StatusCode::BAD_REQUEST, "listing_filter_invalid"));
    }
    let (total, rows) = selected_page(&index, &query);
    // 職種の指定状態の件数（状態以外の絞り込みは反映）。画面のタブと進み具合に使う。
    let mut counts = (0usize, 0usize, 0usize);
    for row in index
        .records
        .iter()
        .filter(|r| matches_except_state(r, &query))
    {
        match row.indeed_title_state {
            "set" => counts.0 += 1,
            "none" => counts.1 += 1,
            _ => counts.2 += 1,
        }
    }
    let publication_counts = publication_counts(&index.records, &query);
    let next_offset = query
        .offset
        .checked_add(rows.len())
        .filter(|next| *next < total);
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "publication_statuses": index.records.iter().filter_map(|r| r.publication_status.clone()).collect::<BTreeSet<_>>(),
            "status":"ready", "listings":rows, "total":total, "index_built_at":index.built_at,
            "offset":query.offset, "next_offset":next_offset, "titles":index.titles,
            "publication_counts": publication_counts,
            "indeed_title_counts":{"set":counts.0,"none":counts.1,"unset":counts.2},
            "refreshing":refreshing, "refresh_failed":refresh_failed, "deals":deal_candidates(&index, query.deal_search.as_deref().unwrap_or("")), "no_deal_count":index.records.iter().filter(|r| r.deal_ids.is_empty()).count()
        })),
    ))
}
/// Re-read acquired counts after index preparation, without fetching bodies or HubSpot.
pub(super) async fn read_applications(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let cached = service(&access)?.listing_index.state.read().await;
    let application_count = cached
        .snapshot
        .as_ref()
        .and_then(|index| index.records.iter().find(|row| row.id == id))
        .and_then(|row| row.indexed_application_count);
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"listing_id":id,"application_count":application_count})),
    ))
}
/// The applications of one listing by application day (JST), for the listing detail. Separate from
/// `read_versions` because it reads every application record. `total` 0 is a real answer (no
/// applicants), not an unknown.
pub(super) async fn read_application_dates(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let service = service(&access)?;
    let applications = service.all_associations("0-420", &id, "0-421").await?;
    let mut properties = vec![
        "yingmuri",
        "seibetsu",
        "nenrei",
        "todoufuken",
        "shikuchouson",
    ];
    properties.extend(super::applicant_reasons::PROPERTIES);
    let pending_labels = service.reason_option_labels();
    let rows = service.batch("0-421", &applications, &properties).await?;
    let labels = match pending_labels {
        super::PendingLabels::Ready(labels) => labels,
        super::PendingLabels::Reading(mut handle) => {
            tokio::time::timeout(super::LABEL_GRACE, &mut handle)
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
        }
    };
    let days: Vec<Option<String>> = rows
        .iter()
        .map(|r| r.value("yingmuri").and_then(super::deal_summary::jst_day))
        .collect();
    let (by_date, missing) = super::deal_summary::count_by_date(&days);
    let people: Vec<Value> = rows
        .iter()
        .zip(&days)
        .take(APPLICATION_ROWS_MAX)
        .map(|(r, day)| {
            json!({"key": super::applicant_reasons::applicant_key(&id, &r.id), "date": day,
                "gender": r.value("seibetsu"), "age": r.value("nenrei"),
                "prefecture": r.value("todoufuken"), "municipality": r.value("shikuchouson")})
        })
        .collect();
    let reasons = super::applicant_reasons::extract_with_labels(
        &id,
        &rows,
        chrono::Utc::now().to_rfc3339(),
        labels.as_ref(),
        super::applicant_reasons::OptionLabelsStatus::Unavailable,
    );
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(
            json!({"listing_id":id,"total":rows.len(),"missing_date":missing,"by_date":by_date,
            "applications":people,"applicant_reasons":reasons,
            "fetched_at":chrono::Utc::now().to_rfc3339()}),
        ),
    ))
}
const APPLICATION_ROWS_MAX: usize = 2000;
/// The history of the status properties (公開状況), oldest first, for the listing's 出来事.
fn status_history(data: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for key in [
        "kyuujin_status",
        "baitai_genjoukyou_hrhakkaa",
        "baitai_genjoukyou_airwork",
    ] {
        let mut entries: Vec<Value> = data["propertiesWithHistory"][key]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter(|e| e["timestamp"].is_string())
                    .map(|e| json!({"timestamp": e["timestamp"], "value": e["value"].as_str().unwrap_or("")}))
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by(|a, b| a["timestamp"].as_str().cmp(&b["timestamp"].as_str()));
        out.insert(key.to_owned(), Value::Array(entries));
    }
    Value::Object(out)
}
#[cfg(test)]
fn association_count(data: &Value, id: &str) -> Option<usize> {
    if data
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty())
    {
        return None;
    }
    let result = data["results"]
        .as_array()?
        .iter()
        .find(|v| v.pointer("/from/id").and_then(Value::as_str) == Some(id))?;
    if result.get("paging").is_some() {
        return None;
    }
    let mut ids = BTreeSet::new();
    for target in result["to"].as_array()? {
        let id = target["toObjectId"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| target["toObjectId"].as_u64().map(|v| v.to_string()))?;
        valid_id(&id).ok()?;
        ids.insert(id);
    }
    Some(ids.len())
}
#[derive(Clone, Debug, Serialize)]
struct Version {
    written_at: String,
    body: String,
    image_urls: Option<Vec<String>>,
    images_registered: Option<bool>,
}
#[derive(Deserialize)]
struct History {
    timestamp: String,
    value: String,
}
fn images_registered_value(value: &str) -> bool {
    value.split_whitespace().any(|part| {
        let reference = if part.starts_with("画像") {
            part.split_once(['：', ':'])
                .map_or(part, |(_, value)| value)
        } else {
            part
        };
        !matches!(reference.trim(), "" | "なし" | "画像なし" | "未登録")
    })
}
/// AirWork image field: (urls in order, registered). Empty or unrecognised text is unknown,
/// never "no images"; only the explicit marker line means the page was checked and had none.
type AwImages = (Option<Vec<String>>, Option<bool>);
fn aw_images(value: &str) -> AwImages {
    let lines: Vec<&str> = value
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines == [AW_NO_IMAGES] {
        return (Some(vec![]), Some(false));
    }
    let urls: Vec<String> = lines
        .iter()
        .filter(|l| l.starts_with("https://") && reqwest::Url::parse(l).is_ok())
        .take(AW_IMAGE_LIMIT)
        .map(|l| (*l).to_owned())
        .collect();
    if urls.is_empty() {
        (None, None)
    } else {
        (Some(urls), Some(true))
    }
}
fn current_image_registration(record: &Record, media: &str) -> Option<bool> {
    if media == "airwork" {
        return record.value(AW_IMAGES).and_then(|v| aw_images(v).1);
    }
    if record
        .value(super::super::job_copy_image_bridge::PROPERTY)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Some(true);
    }
    if media == "hrh" {
        record
            .value("hrh_kyuujinhyou_gazou")
            .map(images_registered_value)
    } else {
        None
    }
}
pub(super) fn image_urls(value: &str) -> Vec<String> {
    // Image fields may contain labelled lines; retain slot order and allow only web URLs.
    value
        .split_whitespace()
        .filter_map(|part| {
            part.find("https://")
                .or_else(|| part.find("http://"))
                .map(|i| &part[i..])
        })
        .filter(|part| reqwest::Url::parse(part).is_ok())
        .map(str::to_owned)
        .take(3)
        .collect()
}
fn versions(
    data: &Value,
    media: &str,
) -> Result<(Vec<Version>, BTreeMap<String, usize>, bool), ReadError> {
    let properties = if media == "hrh" {
        vec!["hrh_kyuujinhyou_honbun", "hrh_kyuujinhyou_gazou"]
    } else {
        vec!["shigotonaiyou", AW_IMAGES]
    };
    let mut counts = BTreeMap::new();
    let mut events: BTreeMap<chrono::DateTime<chrono::Utc>, BTreeMap<&str, String>> =
        BTreeMap::new();
    for property in &properties {
        let values: Vec<History> = match data["propertiesWithHistory"].get(property) {
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|_| fail("hubspot_invalid_history"))?,
            None => vec![],
        };
        counts.insert((*property).to_owned(), values.len());
        for entry in values {
            let time = chrono::DateTime::parse_from_rfc3339(&entry.timestamp)
                .map_err(|_| fail("hubspot_invalid_history"))?
                .with_timezone(&chrono::Utc);
            let at = events.entry(time).or_default();
            if at.get(property).is_some_and(|v| v != &entry.value) {
                return Err(fail("hubspot_ambiguous_history"));
            }
            at.insert(property, entry.value);
        }
    }
    let body_property = properties[0];
    let mut body: Option<String> = None;
    let mut images = None;
    let mut images_registered = None;
    let mut result: Vec<Version> = Vec::new();
    for (time, changes) in events {
        if let Some(value) = changes.get(body_property) {
            body = Some(value.clone());
        }
        if let Some(value) = changes.get("hrh_kyuujinhyou_gazou") {
            images = Some(image_urls(value));
            images_registered = Some(images_registered_value(value));
        }
        if let Some(value) = changes.get(AW_IMAGES) {
            (images, images_registered) = aw_images(value);
        }
        if let Some(body) = &body {
            if result.last().is_some_and(|last| {
                &last.body == body
                    && last.image_urls == images
                    && last.images_registered == images_registered
            }) {
                continue;
            }
            result.push(Version {
                written_at: time.to_rfc3339(),
                body: body.clone(),
                image_urls: images.clone(),
                images_registered,
            });
        }
    }
    let may_be_incomplete = counts.values().any(|count| *count >= 20);
    Ok((result, counts, may_be_incomplete))
}
fn current_body(record: &Record, media: &str) -> Option<Value> {
    let property = if media == "hrh" {
        "hrh_kyuujinhyou_honbun"
    } else {
        "shigotonaiyou"
    };
    let body = record.value(property)?;
    let (images, registered) = if media == "hrh" {
        (
            record.value("hrh_kyuujinhyou_gazou").map(image_urls),
            record
                .value("hrh_kyuujinhyou_gazou")
                .map(images_registered_value),
        )
    } else {
        record.value(AW_IMAGES).map_or((None, None), aw_images)
    };
    Some(
        json!({"checked_at":chrono::Utc::now().to_rfc3339(),"body":body,"image_urls":images,"images_registered":registered}),
    )
}
// Available observed images can accompany an AirWork body; never relabel them as historical images.
async fn observed_images(record: &Record, access: &Access) -> Option<Value> {
    use super::super::job_copy_image_bridge::{Pointer, PROPERTY};

    let bridge = access.images.as_ref()?;
    let pointer: Pointer = serde_json::from_str(record.value(PROPERTY)?).ok()?;
    pointer.validate().ok()?;
    let manifest = bridge.manifest(&pointer).await.ok()?;
    if manifest.listing_id != record.id {
        return None;
    }
    let company = manifest.company_ids.first()?;
    bridge.authorize(company, &record.id).await.ok()?;
    let mut ordered: Vec<_> = manifest.images.iter().collect();
    ordered.sort_by_key(|image| image.slot);
    let urls: Vec<_> = ordered
        .iter()
        .map(|image| {
            format!(
                "/api/job-copy/image?company_id={company}&listing_id={}&manifest_id={}&slot={}",
                record.id, pointer.file_id, image.slot
            )
        })
        .collect();
    Some(json!({"observed_at":manifest.observed_at,"image_urls":urls}))
}
// Read only on detail open. Resolve all related deals in one batch; do not guess
// the customer when several companies are related or any association is incomplete.
async fn listing_company_name(
    service: &JobReadService,
    id: &str,
) -> Result<Option<String>, ReadError> {
    let deals = service.all_associations("0-420", id, "deals").await?;
    if deals.is_empty() || deals.len() > 100 {
        return Ok(None);
    }
    let related = service
        .association_map("deals", &deals, "companies")
        .await?;
    let companies: BTreeSet<String> = related.into_values().flatten().collect();
    if companies.len() != 1 {
        return Ok(None);
    }
    let records = service
        .batch(
            "companies",
            &companies.into_iter().collect::<Vec<_>>(),
            &["name"],
        )
        .await?;
    Ok(records
        .first()
        .and_then(|record| record.value("name"))
        .map(str::to_owned))
}
/// The same answer as `listing_company_name`, for many listings at once: one batch association
/// read (listing to deal), one (deal to company) and one company batch read, instead of the
/// same three reads per listing. A listing is blank (`None`) under the same rules as the
/// single read: no deal, more than 100 deals, or not exactly one related company.
async fn listing_company_names(
    service: &JobReadService,
    ids: &[String],
) -> Result<BTreeMap<String, Option<String>>, ReadError> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let deals_of = service.association_map("0-420", ids, "deals").await?;
    let usable = |deals: &Vec<String>| !deals.is_empty() && deals.len() <= 100;
    let all_deals: Vec<String> = deals_of
        .values()
        .filter(|deals| usable(deals))
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let companies_of = service
        .association_map("deals", &all_deals, "companies")
        .await?;
    let mut company_of: BTreeMap<String, Option<String>> = BTreeMap::new();
    for id in ids {
        let companies: BTreeSet<&String> = match deals_of.get(id) {
            Some(deals) if usable(deals) => deals
                .iter()
                .flat_map(|deal| companies_of.get(deal).into_iter().flatten())
                .collect(),
            _ => BTreeSet::new(),
        };
        company_of.insert(
            id.clone(),
            (companies.len() == 1)
                .then(|| companies.into_iter().next().cloned().unwrap_or_default()),
        );
    }
    let wanted: Vec<String> = company_of
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let names: BTreeMap<String, Option<String>> = service
        .batch("companies", &wanted, &["name"])
        .await?
        .into_iter()
        .map(|record| {
            let name = record.value("name").map(str::to_owned);
            (record.id, name)
        })
        .collect();
    Ok(company_of
        .into_iter()
        .map(|(id, company)| {
            let name = company.and_then(|company| names.get(&company).cloned().flatten());
            (id, name)
        })
        .collect())
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FreshQuery {
    fresh: Option<String>,
}
impl FreshQuery {
    fn requested(&self) -> bool {
        matches!(self.fresh.as_deref(), Some("1" | "true"))
    }
}
/// The listing record with its history. Reused for 60 seconds across users; `fresh=1` rereads.
async fn listing_data(
    service: &JobReadService,
    id: &str,
    fresh: bool,
) -> Result<(Arc<Value>, Option<u64>), ReadError> {
    let epoch = service.detail_cache.epoch();
    if !fresh {
        if let Some(data) = service.detail_cache.detail.get(id) {
            return Ok((data, None));
        }
    }
    let data = service
        .request(
            &format!("/crm/v3/objects/0-420/{id}"),
            &[
                (
                    "properties",
                    format!(
                        "{},{},{},{},{}",
                        PROPERTIES.join(","),
                        INDEED_WEEKLY_PERF,
                        AIRWORK_WEEKLY_PERF,
                        super::super::job_copy_image_bridge::PROPERTY,
                        crate::job_gen::drafts::PROPERTIES.join(",")
                    ),
                ),
                (
                    "propertiesWithHistory",
                    format!(
                        "hrh_kyuujinhyou_honbun,hrh_kyuujinhyou_gazou,shigotonaiyou,kyuujin_status,baitai_genjoukyou_hrhakkaa,baitai_genjoukyou_airwork,{AW_IMAGES},{}",
                        crate::job_gen::drafts::FACTS
                    ),
                ),
            ],
            None,
        )
        .await?;
    Ok((Arc::new(data), Some(epoch)))
}
pub(super) async fn read_versions(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(id): AxumPath<String>,
    Query(fresh): Query<FreshQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let (data_arc, fetched_at_epoch) =
        listing_data(service(&access)?, &id, fresh.requested()).await?;
    let data = &*data_arc;
    let record: Record =
        serde_json::from_value(data.clone()).map_err(|_| fail("hubspot_invalid_response"))?;
    if record.id != id {
        return Err(fail("hubspot_invalid_response"));
    }
    state.indeed_db.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "market_data_unavailable",
    ))?;
    let row = listing(&record).ok_or(ReadError(
        StatusCode::UNPROCESSABLE_ENTITY,
        "listing_media_unknown",
    ))?;
    let (versions, counts, may_be_incomplete) = versions(data, row.media)?;
    let current = current_body(&record, row.media).filter(|value| {
        versions
            .last()
            .is_none_or(|version| Some(version.body.as_str()) != value["body"].as_str())
    });
    // AirWork images come from `aw_kyuujinhyou_gazou`; the Drive manifest path is not used for it.
    let current_images = if row.media == "airwork" {
        None
    } else {
        observed_images(&record, &access).await
    };
    let current_images_registered = current_image_registration(&record, row.media);
    // Whole answer parsed and checked: only now is a fresh HubSpot read worth keeping. A read
    // served from the cache is not stored again, so the 60 seconds never slide.
    if let Some(epoch) = fetched_at_epoch {
        service(&access)?
            .detail_cache
            .store_detail(&id, data_arc.clone(), epoch);
    }
    let (drafts, draft_history_may_be_incomplete) = crate::job_gen::drafts::history(data);
    // Independent conditions are current values, not historical conditions.
    let current_fields = deals::comparison_fields(&record, row.media);
    let draft_revision = crate::job_gen::drafts::revision(&record.properties);
    let (indeed_perf, airwork_perf) = if row.media == "hrh" {
        (indeed_weekly_perf(&record), None)
    } else {
        (None, airwork_weekly_perf(&record))
    };
    let can_write_drafts = state.audit.is_some() && super::drafts::Writes::from_env().allows(&id);
    // Use the same acquired counts as the list/deal table, across the entire index.
    // An absent index/record/count stays unknown; no additional HubSpot read is needed.
    let application_count = service(&access)?
        .listing_index
        .state
        .read()
        .await
        .snapshot
        .as_ref()
        .and_then(|index| index.records.iter().find(|item| item.id == id))
        .and_then(|item| item.indexed_application_count);

    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(
            json!({"current_fields":current_fields,"current_row": super::manual_edit::current_row(&record.properties), "listing":PageListing {listing: row, application_count},"versions":versions,"history_counts":counts,"history_may_be_incomplete":may_be_incomplete,"current":current,"current_images":current_images,"current_images_registered":current_images_registered,"drafts":drafts,"draft_revision":draft_revision,"can_write_drafts":can_write_drafts,"draft_history_may_be_incomplete":draft_history_may_be_incomplete,"indeed_weekly_perf":indeed_perf,"airwork_weekly_perf":airwork_perf,"listing_started_on":super::listing_status::day(record.value("koukai_kaishi_nichiji")),"status_history":status_history(data)}),
        ),
    ))
}
/// The customer name for the detail header. Separate from `read_versions` because it needs three
/// more HubSpot reads and is only a label: the body and images should not wait for it.
pub(super) async fn read_company(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(id): AxumPath<String>,
    Query(fresh): Query<FreshQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let service = service(&access)?;
    let cached = if fresh.requested() {
        None
    } else {
        service.detail_cache.company.get(&id)
    };
    let company_name = match cached {
        Some(name) => name,
        None => {
            // A failure is an error for the caller (it just leaves the name blank) and is not cached.
            let name = listing_company_name(service, &id).await?;
            service.detail_cache.company.put(&id, name.clone());
            name
        }
    };
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"listing_id":id,"company_name":company_name})),
    ))
}
const COMPANY_BATCH_MAX: usize = 100;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompaniesQuery {
    ids: String,
    fresh: Option<String>,
}
/// Customer names for the listings shown in the list, in a few HubSpot reads instead of three
/// per listing. Same authorization and same names as `read_company`; names already known (6 h
/// cache) are not read again. A failed read is an error and nothing is cached for it.
pub(super) async fn read_companies(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<CompaniesQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let mut ids: Vec<String> = Vec::new();
    for id in query.ids.split(',') {
        valid_id(id)?;
        if !ids.iter().any(|known| known == id) {
            ids.push(id.to_owned());
        }
    }
    if ids.len() > COMPANY_BATCH_MAX {
        return Err(ReadError(StatusCode::BAD_REQUEST, "too_many_ids"));
    }
    let service = service(&access)?;
    let fresh = matches!(query.fresh.as_deref(), Some("1" | "true"));
    let mut names: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut missing = Vec::new();
    for id in &ids {
        match (!fresh)
            .then(|| service.detail_cache.company.get(id))
            .flatten()
        {
            Some(name) => {
                names.insert(id.clone(), name);
            }
            None => missing.push(id.clone()),
        }
    }
    for (id, name) in listing_company_names(service, &missing).await? {
        service.detail_cache.company.put(&id, name.clone());
        names.insert(id, name);
    }
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({ "companies": names })),
    ))
}
/// テスト用: 一覧の行の職種指定（職種名, 状態）。
#[cfg(test)]
pub(super) fn tests_listing(record: &Record) -> (Option<String>, &'static str) {
    let row = listing(record).unwrap();
    (row.indeed_title, row.indeed_title_state)
}
#[cfg(test)]
mod publication_count_tests {
    use super::*;
    fn row(id: &str, media: &'static str, status: Option<&str>) -> Listing {
        Listing {
            id: id.into(),
            media,
            media_job_id: id.into(),
            account_id: None,
            title: None,
            prefecture: None,
            municipality: None,
            category: None,
            indeed_title: None,
            indeed_title_state: "unset",
            publication_status: status.map(str::to_owned),
            draft_status: None,
            last_csv_detected_at: None,
            deal_ids: vec![],
            summary: Summary::default(),
            indexed_application_count: None,
            perf: None,
        }
    }
    #[test]
    fn several_deals_can_be_given_and_a_listing_in_two_deals_counts_once() {
        let mut rows = vec![
            row("1", "hrh", Some("掲載中")),
            row("2", "hrh", Some("掲載中")),
            row("3", "hrh", Some("掲載終了")),
            row("4", "hrh", Some("掲載中")),
        ];
        rows[0].deal_ids = vec!["101".into()];
        rows[1].deal_ids = vec!["101".into(), "102".into()];
        rows[2].deal_ids = vec!["102".into()];
        rows[3].deal_ids = vec!["999".into()];
        let mut q = ListingsQuery::default();
        q.deal = Some("101,102".into());
        let ids: Vec<&str> = rows
            .iter()
            .filter(|r| matches(r, &q))
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(ids, ["1", "2", "3"]);
        assert_eq!(
            publication_counts(&rows, &q),
            json!({"live":2,"ended":1,"before":0,"unknown":0})
        );
        q.deal = Some("102".into());
        assert_eq!(rows.iter().filter(|r| matches(r, &q)).count(), 2);
    }
    #[test]
    fn counts_ignore_the_publication_filter_but_keep_the_others() {
        let rows = vec![
            row("1", "hrh", Some("掲載中")),
            row("2", "hrh", Some("掲載中")),
            row("3", "hrh", Some("掲載終了")),
            row("4", "airwork", Some("掲載終了")),
            row("5", "hrh", Some("掲載前")),
            row("6", "hrh", None),
        ];
        let all = ListingsQuery::default();
        assert_eq!(
            publication_counts(&rows, &all),
            json!({"live":2,"ended":2,"before":1,"unknown":1})
        );
        let mut q = ListingsQuery::default();
        q.publication = Some("掲載中".into());
        q.media = Some("hrh".into());
        // 公開状況の指定は件数に影響せず、媒体の指定は反映される。
        assert_eq!(
            publication_counts(&rows, &q),
            json!({"live":2,"ended":1,"before":1,"unknown":1})
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn job_copy_detail_counts_cover_jobs_outside_the_first_page_and_preserve_unknowns() {
        use axum::{body::to_bytes, routing::any};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let record: Record = serde_json::from_value(json!({"id":"51","properties":{
            "id_hrhakkaa":"synthetic-job","hs_name":"架空の直接リンク求人",
            "hrh_kyuujinhyou_honbun":"仕事内容：棚の品物を確認します。"
        }}))
        .unwrap();
        let upstream_record = record.clone();
        let upstream = Router::new().fallback(any(
            move |request: axum::http::Request<axum::body::Body>| {
                let seen = seen.clone();
                let record = upstream_record.clone();
                async move {
                    assert_eq!(request.method(), reqwest::Method::GET);
                    seen.fetch_add(1, Ordering::Relaxed);
                    match request.uri().path() {
                        "/crm/v3/objects/0-420/51" => {
                            Json(json!({"id":record.id,"properties":record.properties}))
                        }
                        "/crm/v4/objects/0-420/51/associations/deals" => {
                            Json(json!({"results":[]}))
                        }
                        path => panic!("unexpected read: {path}"),
                    }
                }
            },
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service = Arc::new(JobReadService::for_test(format!(
            "http://{}",
            listener.local_addr().unwrap()
        )));
        let task = tokio::spawn(async move {
            axum::serve(listener, upstream).await.unwrap();
        });
        let access = Access {
            allowed: BTreeSet::from(["reader@example.test".into()]),
            service: Some(service.clone()),
            moc_path: None,
            moc_drive: Ok(None),
            snapshot_reader: None,
            snapshot_cache: Arc::default(),
            resolved_jobs: Arc::default(),
            images: None,
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
        };
        let file = tempfile::NamedTempFile::new().unwrap();
        rusqlite::Connection::open(file.path())
            .unwrap()
            .execute_batch("CREATE TABLE insight_title_pref (norm_title TEXT);")
            .unwrap();
        let mut state = super::super::integration_tests::moc_state();
        Arc::get_mut(&mut state).unwrap().indeed_db =
            Some(crate::db::local_sqlite::LocalDb::new(file.path().to_str().unwrap()).unwrap());
        let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        session
            .insert(SESSION_USER_KEY, "reader@example.test")
            .await
            .unwrap();
        session
            .insert(SESSION_LOGIN_METHOD_KEY, LOGIN_METHOD_GOOGLE_OIDC)
            .await
            .unwrap();
        for mode in ["zero", "seven", "unknown", "missing-job", "missing-index"] {
            let count = match mode {
                "zero" => Some(0),
                "seven" => Some(7),
                _ => None,
            };
            let records: Vec<_> = (1..=51)
                .map(|id| {
                    let mut row = listing(&record).unwrap();
                    row.id = id.to_string();
                    row.title = Some(format!("架空求人{id:03}"));
                    row.indexed_application_count = if id == 51 { count } else { Some(99) };
                    row
                })
                .filter(|row| mode != "missing-job" || row.id != "51")
                .collect();
            let index = Arc::new(Index {
                records,
                titles: vec![],
                deals: vec![],
                built_at: "2026-10-12T00:00:00Z".into(),
            });
            let (_, first_page) = selected_page(&index, &ListingsQuery::default());
            assert_eq!(first_page.len(), 50);
            assert!(first_page.iter().all(|row| row.listing.id != "51"));
            service.listing_index.state.write().await.snapshot = if mode == "missing-index" {
                None
            } else {
                Some(index)
            };
            let counts = read_applications(
                State(state.clone()),
                Extension(access.clone()),
                session.clone(),
                AxumPath("51".into()),
            )
            .await
            .unwrap()
            .into_response();
            assert_eq!(counts.headers()[header::CACHE_CONTROL], "private, no-store");
            let counts: Value =
                serde_json::from_slice(&to_bytes(counts.into_body(), 4096).await.unwrap()).unwrap();
            assert_eq!(
                counts,
                json!({"listing_id":"51","application_count":count}),
                "{mode}"
            );
            let response = read_versions(
                State(state.clone()),
                Extension(access.clone()),
                session.clone(),
                AxumPath("51".into()),
                Query(FreshQuery::default()),
            )
            .await
            .unwrap()
            .into_response();
            assert_eq!(response.status(), StatusCode::OK);
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap())
                    .unwrap();
            assert_eq!(body["listing"]["id"], "51");
            assert_eq!(body["listing"]["application_count"], json!(count), "{mode}");
        }
        let anonymous = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        assert!(matches!(
            read_applications(
                State(state.clone()),
                Extension(access.clone()),
                anonymous,
                AxumPath("51".into())
            )
            .await,
            Err(ReadError(StatusCode::UNAUTHORIZED, _))
        ));
        session
            .insert(SESSION_USER_KEY, "outside@example.test")
            .await
            .unwrap();
        assert!(matches!(
            read_applications(
                State(state.clone()),
                Extension(access.clone()),
                session.clone(),
                AxumPath("51".into())
            )
            .await,
            Err(ReadError(StatusCode::FORBIDDEN, _))
        ));
        session
            .insert(SESSION_USER_KEY, "reader@example.test")
            .await
            .unwrap();
        assert!(matches!(
            read_applications(
                State(state),
                Extension(access),
                session,
                AxumPath("not-a-job".into())
            )
            .await,
            Err(ReadError(StatusCode::BAD_REQUEST, _))
        ));
        // One listing read for the first detail request; the other four are served from the
        // 60-second cache (only the counts, computed from the index, differ per mode). The
        // company is no longer read by the detail request at all.
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        task.abort();
    }
    #[tokio::test]
    async fn job_copy_detail_company_is_batched_and_never_guesses_ambiguous_relations() {
        use axum::{body::Body, http::Request, routing::any, Json, Router};
        for mode in ["single", "multiple", "empty", "failure"] {
            let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
            let seen = calls.clone();
            let upstream = Router::new().fallback(any(move |request: Request<Body>| {
                let seen = seen.clone();
                async move {
                    let path = request.uri().path().to_owned();
                    seen.lock().unwrap().push(path.clone());
                    match path.as_str() {
                        "/crm/v4/objects/0-420/30/associations/deals" => {
                            assert_eq!(request.method(), reqwest::Method::GET);
                            if mode == "failure" {
                                return (StatusCode::FORBIDDEN, Json(json!({}))).into_response();
                            }
                            Json(if mode == "empty" { json!({"results":[]}) } else {
                                json!({"results":[{"toObjectId":"20"},{"toObjectId":"21"}]})
                            }).into_response()
                        }
                        "/crm/v4/associations/deals/companies/batch/read" => {
                            assert_eq!(request.method(), reqwest::Method::POST);
                            let body = axum::body::to_bytes(request.into_body(), 4096).await.unwrap();
                            let body: Value = serde_json::from_slice(&body).unwrap();
                            assert_eq!(body["inputs"], json!([{"id":"20"},{"id":"21"}]));
                            Json(json!({"results":[
                                {"from":{"id":"20"},"to":[{"toObjectId":"10"}]},
                                {"from":{"id":"21"},"to":[{"toObjectId":if mode == "multiple" {"11"} else {"10"}}]}
                            ]})).into_response()
                        }
                        "/crm/v3/objects/companies/batch/read" => {
                            let body = axum::body::to_bytes(request.into_body(), 4096).await.unwrap();
                            let body: Value = serde_json::from_slice(&body).unwrap();
                            assert_eq!(body["inputs"], json!([{"id":"10"}]));
                            assert_eq!(body["properties"], json!(["name"]));
                            Json(json!({"results":[{"id":"10","properties":{"name":"合成運輸株式会社"}}]})).into_response()
                        }
                        _ => panic!("unexpected company read: {path}"),
                    }
                }
            }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let service =
                JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
            let task = tokio::spawn(async move {
                axum::serve(listener, upstream).await.unwrap();
            });
            let result = listing_company_name(&service, "30").await;
            if mode == "failure" {
                assert!(result.is_err());
            } else {
                assert_eq!(
                    result.unwrap().as_deref(),
                    if mode == "single" {
                        Some("合成運輸株式会社")
                    } else {
                        None
                    }
                );
            }
            assert_eq!(
                calls.lock().unwrap().len(),
                match mode {
                    "single" => 3,
                    "multiple" => 2,
                    _ => 1,
                }
            );
            task.abort();
        }
    }
    #[test]
    fn job_copy_multiline_occupation_is_classified_and_filtered() {
        let record: Record = serde_json::from_value(json!({"id":"1","properties":{"id_hrhakkaa":"HR-1","hrh_kyuujinhyou_honbun":"仕事内容：配送します\nIndeed表示職種名：\n配送\nドライバー\n応募資格：普通免許","todoufuken":"沖縄県","jobcopy_indeed_title":"配送ドライバー"}})).unwrap();
        let titles = vec!["配送ドライバー".into()];
        let row = listing(&record).unwrap();
        assert_eq!(row.category.as_deref(), Some("配送ドライバー"));
        let index = Index {
            records: vec![row],
            titles,
            built_at: "2026-10-10T00:00:00Z".into(),
            deals: vec![],
        };
        let (total, page) = selected_page(
            &index,
            &ListingsQuery {
                prefecture: Some("沖縄県".into()),
                title: Some("配送ドライバー".into()),
                media: Some("hrh".into()),
                ..Default::default()
            },
        );
        assert_eq!(total, 1);
        assert_eq!(page[0].listing.id, "1");
    }
    #[test]
    fn job_copy_bulk_occupation_filter_search_and_order() {
        let mk = |id: &str, title: &str, status: &str, apps: usize, prop: &str| {
            let mut properties =
                json!({"id_hrhakkaa":format!("HR-{id}"),"hs_name":title,"kyuujin_status":status});
            if !prop.is_empty() {
                properties["jobcopy_indeed_title"] = json!(prop);
            }
            let record: Record =
                serde_json::from_value(json!({"id":id,"properties":properties})).unwrap();
            let mut row = listing(&record).unwrap();
            row.indexed_application_count = Some(apps);
            row
        };
        let index = Index {
            records: vec![
                mk("1", "配送A", "公開中", 3, ""),
                mk("2", "配送B", "公開中", 9, "配送ドライバー"),
                mk("3", "事務C", "公開中", 9, "__none__"),
                mk("4", "配送D", "公開終了", 50, ""),
                mk("5", "配送E", "公開中", 7, ""),
            ],
            titles: vec!["配送ドライバー".into()],
            built_at: "2026-10-10T00:00:00Z".into(),
            deals: vec![],
        };
        let ids = |q: ListingsQuery| {
            selected_page(&index, &q)
                .1
                .into_iter()
                .map(|r| r.listing.id)
                .collect::<Vec<_>>()
        };
        let q = |sort: Option<&str>, state: Option<&str>, text: Option<&str>| ListingsQuery {
            sort: sort.map(Into::into),
            indeed_state: state.map(Into::into),
            q: text.map(Into::into),
            ..Default::default()
        };
        // 掲載中が先、応募の多い順、同数は番号順。掲載終了(50件)は最後。
        assert_eq!(ids(q(Some("bulk"), None, None)), ["2", "3", "5", "1", "4"]);
        assert_eq!(ids(q(Some("bulk"), Some("unset"), None)), ["5", "1", "4"]);
        assert_eq!(ids(q(Some("bulk"), Some("assigned"), None)), ["2", "3"]);
        assert_eq!(
            ids(q(Some("bulk"), Some("unset"), Some("配送"))),
            ["5", "1", "4"]
        );
        assert_eq!(ids(q(Some("bulk"), None, Some("事務"))), ["3"]);
    }
    #[test]
    fn job_copy_prefecture_specific_values() {
        assert_eq!(
            location(" 大分県大分市中央町1-1 "),
            (Some("大分県".into()), Some("大分市".into()))
        );
        assert_eq!(
            location("東京都新宿区西新宿"),
            (Some("東京都".into()), Some("新宿区".into()))
        );
        assert_eq!(location("架空県架空市"), (None, None));
    }
    #[test]
    fn job_copy_history_order_duplicates_and_images() {
        let data = json!({"propertiesWithHistory": {
            "hrh_kyuujinhyou_honbun":[{"timestamp":"2026-10-03T00:00:00Z","value":"新本文"},{"timestamp":"2026-10-01T00:00:00Z","value":"旧本文"},{"timestamp":"2026-10-02T00:00:00Z","value":"旧本文"}],
            "hrh_kyuujinhyou_gazou":[{"timestamp":"2026-10-02T12:00:00Z","value":"画像1：https://example.invalid/1.png\n画像2：なし\n画像3：https://example.invalid/3.png"},{"timestamp":"2026-10-01T00:00:00Z","value":"なし"}]
        }});
        let (rows, counts, incomplete) = versions(&data, "hrh").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].written_at, "2026-10-01T00:00:00+00:00");
        assert_eq!(rows[0].body, "旧本文");
        assert_eq!(rows[0].image_urls, Some(vec![]));
        assert_eq!(
            rows[1].image_urls,
            Some(vec![
                "https://example.invalid/1.png".into(),
                "https://example.invalid/3.png".into()
            ])
        );
        assert_eq!(rows[2].body, "新本文");
        assert_eq!(counts["hrh_kyuujinhyou_honbun"], 3);
        assert!(!incomplete);
        let (aw, _, _) = versions(&json!({"propertiesWithHistory":{"shigotonaiyou":[{"timestamp":"2026-10-01T00:00:00Z","value":"合成仕事内容"}]}}), "airwork").unwrap();
        assert_eq!(aw[0].body, "合成仕事内容");
        assert_eq!(aw[0].image_urls, None);
    }
    #[test]
    fn job_copy_airwork_images_use_their_own_field_in_order_and_history() {
        let urls: Vec<String> = (1..=6)
            .map(|i| format!("https://cdn.example.invalid/p/{i}.jpg"))
            .collect();
        let raw = urls.join("\n");
        let (list, registered) = aw_images(&format!("{raw}\n"));
        assert_eq!(list, Some(urls.clone()));
        assert_eq!(registered, Some(true));
        assert_eq!(aw_images("NO_IMAGES\n"), (Some(vec![]), Some(false)));
        assert_eq!(aw_images(""), (None, None));
        assert_eq!(aw_images("画像なし"), (None, None));
        assert_eq!(
            aw_images("NO_IMAGES\nhttps://cdn.example.invalid/x.jpg").1,
            Some(true)
        );
        let data = json!({"propertiesWithHistory": {
            "shigotonaiyou":[{"timestamp":"2026-10-01T00:00:00Z","value":"本文"}],
            "aw_kyuujinhyou_gazou":[{"timestamp":"2026-10-02T00:00:00Z","value":"NO_IMAGES"},{"timestamp":"2026-10-03T00:00:00Z","value":raw}],
            "hrh_kyuujinhyou_gazou":[{"timestamp":"2026-10-02T00:00:00Z","value":"https://hrh.example.invalid/other.png"}]
        }});
        let (rows, counts, _) = versions(&data, "airwork").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].image_urls, None);
        assert_eq!(rows[1].image_urls, Some(vec![]));
        assert_eq!(rows[1].images_registered, Some(false));
        assert_eq!(rows[2].image_urls, Some(urls.clone()));
        assert_eq!(counts["aw_kyuujinhyou_gazou"], 2);
        assert!(!counts.contains_key("hrh_kyuujinhyou_gazou"));
        // A listing carrying both media fields: each media reads only its own field.
        let both: Record = serde_json::from_value(json!({"id":"7","properties":{
            "id_airwork":"AW-7","shigotonaiyou":"本文","aw_kyuujinhyou_gazou":raw,
            "hrh_kyuujinhyou_honbun":"hrh本文","hrh_kyuujinhyou_gazou":"https://hrh.example.invalid/other.png"}})).unwrap();
        let aw = current_body(&both, "airwork").unwrap();
        assert_eq!(aw["image_urls"].as_array().unwrap().len(), 6);
        assert_eq!(aw["image_urls"][5], "https://cdn.example.invalid/p/6.jpg");
        assert_eq!(aw["images_registered"], true);
        let hrh = current_body(&both, "hrh").unwrap();
        assert_eq!(
            hrh["image_urls"],
            json!(["https://hrh.example.invalid/other.png"])
        );
        assert_eq!(current_image_registration(&both, "airwork"), Some(true));
        let none: Record = serde_json::from_value(json!({"id":"8","properties":{"id_airwork":"AW-8","shigotonaiyou":"本文","aw_kyuujinhyou_gazou":"NO_IMAGES","hrh_kyuujinhyou_gazou":"https://hrh.example.invalid/other.png"}})).unwrap();
        assert_eq!(current_image_registration(&none, "airwork"), Some(false));
        let nothing: Record = serde_json::from_value(
            json!({"id":"9","properties":{"id_airwork":"AW-9","shigotonaiyou":"本文"}}),
        )
        .unwrap();
        assert_eq!(
            current_body(&nothing, "airwork").unwrap()["image_urls"],
            Value::Null
        );
        assert_eq!(current_image_registration(&nothing, "airwork"), None);
    }
    #[test]
    fn job_copy_image_registration_retains_filenames_and_distinguishes_absence() {
        assert!(!images_registered_value(""));
        assert!(!images_registered_value("なし"));
        assert!(!images_registered_value(
            "画像1：なし\n画像2：\n画像3：未登録"
        ));
        assert!(images_registered_value("画像1：warehouse.jpg\n画像2：なし"));
        assert!(images_registered_value(
            "https://example.invalid/warehouse.jpg"
        ));
        let data = json!({"propertiesWithHistory": {
            "hrh_kyuujinhyou_honbun":[{"timestamp":"2026-10-01T00:00:00Z","value":"本文"}],
            "hrh_kyuujinhyou_gazou":[
                {"timestamp":"2026-10-01T00:00:00Z","value":"warehouse.jpg"},
                {"timestamp":"2026-10-02T00:00:00Z","value":""}
            ]
        }});
        let (rows, _, _) = versions(&data, "hrh").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].image_urls, Some(vec![]));
        assert_eq!(rows[0].images_registered, Some(true));
        assert_eq!(rows[1].images_registered, Some(false));
    }
    #[test]
    fn indeed_weekly_perf_is_read_only_when_well_formed() {
        let rec = |v: serde_json::Value| -> Record {
            serde_json::from_value(json!({"id":"1","properties":{"indeed_weekly_perf":v}})).unwrap()
        };
        let good = r#"{"schema_version":1,"source":"hrhacker","job_id":"1234567","captured_at":"2026-10-14T00:00:00Z","rows":[{"period_start":"2026-10-05","period_end":"2026-10-11","impressions":10,"clicks":2,"cost_yen":1500,"applications":1}]}"#;
        let value = indeed_weekly_perf(&rec(json!(good))).unwrap();
        assert_eq!(value["rows"][0]["cost_yen"], 1500);
        assert!(indeed_weekly_perf(&rec(json!("not json"))).is_none());
        assert!(indeed_weekly_perf(&rec(json!(
            r#"{"schema_version":2,"source":"hrhacker","rows":[]}"#
        )))
        .is_none());
        assert!(indeed_weekly_perf(&rec(json!(null))).is_none());
        assert!(indeed_weekly_perf(&rec(json!(" "))).is_none());
    }
    #[test]
    fn airwork_weekly_perf_needs_matching_digit_job_id() {
        let rec = |id: serde_json::Value, v: &str| -> Record {
            serde_json::from_value(
                json!({"id":"1","properties":{"id_airwork":id,"airwork_weekly_perf":v}}),
            )
            .unwrap()
        };
        let body = |source: &str, job: &str| {
            format!(
                r#"{{"schema_version":1,"source":"{source}","job_id":"{job}","captured_at":"2026-10-14T00:00:00Z","rows":[{{"period_start":"2026-10-05","period_end":"2026-10-11","impressions":10,"clicks":2,"cost_yen":2500,"applications":1}}]}}"#
            )
        };
        let ok = airwork_weekly_perf(&rec(json!("98765"), &body("airwork", "98765"))).unwrap();
        assert_eq!(ok["rows"][0]["cost_yen"], 2500);
        // other listing's id, wrong source, non-digit id, bad json: all ignored
        assert!(airwork_weekly_perf(&rec(json!("98765"), &body("airwork", "11111"))).is_none());
        assert!(airwork_weekly_perf(&rec(json!("98765"), &body("hrhacker", "98765"))).is_none());
        assert!(airwork_weekly_perf(&rec(json!("AW-1"), &body("airwork", "AW-1"))).is_none());
        assert!(airwork_weekly_perf(&rec(json!("98765"), "{oops")).is_none());
    }
    #[test]
    fn job_copy_listing_publication_rebuild_and_filter() {
        assert!(PROPERTIES.contains(&"kyuujin_status"));
        for (media, normalized, raw, expected) in [
            ("airwork", "", "02", "掲載中"),
            ("airwork", "", "01", "掲載終了"),
            ("airwork", "公開前", "02", "掲載前"),
            ("airwork", "公開中", "01", "掲載中"),
            ("hrh", "公開終了", "公開", "掲載終了"),
            ("hrh", "", "公開", "掲載中"),
            ("hrh", "", "", "不明"),
        ] {
            let record: Record = serde_json::from_value(json!({"id":"42","properties":{
                if media == "hrh" { "id_hrhakkaa" } else { "id_airwork" }: "42",
                "kyuujin_status": normalized,
                if media == "hrh" { "baitai_genjoukyou_hrhakkaa" } else { "baitai_genjoukyou_airwork" }: raw
            }})).unwrap();
            let row = listing(&record).unwrap();
            assert_eq!(row.publication_status.as_deref(), Some(expected));
            assert!(matches(
                &row,
                &ListingsQuery {
                    publication: Some(expected.into()),
                    ..Default::default()
                }
            ));
            assert_eq!(
                matches(
                    &row,
                    &ListingsQuery {
                        publication: Some("unknown".into()),
                        ..Default::default()
                    }
                ),
                expected == "不明"
            );
        }
    }
    #[test]
    fn job_copy_media_keys_and_unknown_values() {
        let make = |id: &str, account: &str| {
            serde_json::from_value::<Record>(json!({"id":id,"properties":{"id_airwork":"same-id","airwork_account_login_id":account,"qinwude":"千葉県市川市合成勤務地","zhizhong":"運転手"}})).unwrap()
        };
        let first = listing(&make("10", "synthetic-a")).unwrap();
        let second = listing(&make("11", "synthetic-b")).unwrap();
        assert_eq!(first.media_job_id, second.media_job_id);
        assert_ne!(first.account_id, second.account_id);
        assert_eq!(first.prefecture, Some("千葉県".into()));
        assert_eq!(first.municipality, Some("市川市".into()));
        assert_eq!(first.category, None);
        assert_eq!(first.publication_status.as_deref(), Some("不明"));
        let mut published = make("12", "synthetic-c");
        published
            .properties
            .insert("baitai_genjoukyou_airwork".into(), Some("掲載中".into()));
        assert_eq!(
            listing(&published).unwrap().publication_status,
            Some("掲載中".into())
        );

        assert_eq!(image_urls("画像1：https://example.invalid/img,a.png 画像2：なし 画像3：https://example.invalid/3.png"), vec!["https://example.invalid/img,a.png", "https://example.invalid/3.png"]);
    }
    #[test]
    fn job_copy_status_history_is_sorted_and_keeps_only_status_fields() {
        let data = json!({"propertiesWithHistory": {
            "baitai_genjoukyou_hrhakkaa": [
                {"timestamp": "2026-08-17T03:00:00Z", "value": "公開"},
                {"timestamp": "2026-06-15T00:00:00Z", "value": "公開"},
                {"timestamp": "2026-08-03T03:00:00Z", "value": "非公開"}],
            "shigotonaiyou": [{"timestamp": "2026-06-15T00:00:00Z", "value": "本文"}]}});
        let history = status_history(&data);
        let values: Vec<&str> = history["baitai_genjoukyou_hrhakkaa"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["value"].as_str().unwrap())
            .collect();
        assert_eq!(values, vec!["公開", "非公開", "公開"]);
        assert_eq!(
            history["baitai_genjoukyou_hrhakkaa"][0]["timestamp"],
            "2026-06-15T00:00:00Z"
        );
        assert_eq!(history["kyuujin_status"], json!([]));
        assert!(history.get("shigotonaiyou").is_none());
    }
    #[test]
    fn job_copy_counts_do_not_invent_zero() {
        assert_eq!(association_count(&json!({"results":[]}), "10"), None);
        assert_eq!(
            association_count(&json!({"results":[{"from":{"id":"10"},"to":[]}]}), "10"),
            Some(0)
        );
        assert_eq!(
            association_count(
                &json!({"results":[{"from":{"id":"10"},"to":[{"toObjectId":21},{"toObjectId":21},{"toObjectId":22}]}]}),
                "10"
            ),
            Some(2)
        );
        assert_eq!(
            association_count(
                &json!({"results":[{"from":{"id":"10"},"to":[],"paging":{}}]}),
                "10"
            ),
            None
        );
    }
    #[tokio::test]
    async fn job_copy_index_three_complete_pages_filter_offset_and_failed_refresh() {
        use axum::{body::Body, http::Request, routing::any};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        };
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let failed = Arc::new(AtomicBool::new(false));
        let calls = seen.clone();
        let fail_next = failed.clone();
        let app = Router::new().fallback(any(move |req: Request<Body>| {
            let calls = calls.clone(); let fail_next = fail_next.clone();
            async move {
                let path = req.uri().path().to_owned();
                if path.contains("/associations/") {
                    calls.lock().unwrap().push(path.clone());
                    let bytes = axum::body::to_bytes(req.into_body(), 100000).await.unwrap();
                    let input: Value = serde_json::from_slice(&bytes).unwrap();
                    // Like HubSpot: objects without associations come back as NO_ASSOCIATIONS_FOUND errors.
                    let (mut results, mut empty) = (vec![], vec![]);
                    for v in input["inputs"].as_array().unwrap() {
                        let id = v["id"].as_str().unwrap();
                        let targets = if path.contains("/deals/") { match id { "1" => vec![json!({"toObjectId":901}),json!({"toObjectId":902}),json!({"toObjectId":903})], "2" => vec![json!({"toObjectId":901})], _ => vec![] } } else { vec![json!({"toObjectId":7001}),json!({"toObjectId":7002})] };
                        if targets.is_empty() { empty.push(json!(id)); } else { results.push(json!({"from":{"id":id},"to":targets})); }
                    }
                    let errors: Vec<Value> = empty.chunks(50).map(|ids| json!({"status":"error","category":"OBJECT_NOT_FOUND","subCategory":"crm.associations.NO_ASSOCIATIONS_FOUND","context":{"fromObjectId":ids,"fromObjectType":["listing"],"toObjectType":["deal"]}})).collect();
                    return Json(json!({"status":"COMPLETE","results":results,"errors":errors})).into_response();
                }
                if path == "/crm/v3/objects/0-420/batch/read" {
                    let bytes = axum::body::to_bytes(req.into_body(), 100000).await.unwrap();
                    let input: Value = serde_json::from_slice(&bytes).unwrap();
                    return Json(json!({"results":input["inputs"].as_array().unwrap().iter().map(|v| json!({"id":v["id"],"properties":{}})).collect::<Vec<_>>()})).into_response();
                }
                if path == "/crm/v3/objects/deals/batch/read" {
                    calls.lock().unwrap().push(path);
                    return Json(json!({"results":[{"id":"901","properties":{"dealname":"沖縄配送・契約A","pipeline":"21596025"}},{"id":"902","properties":{"dealname":"沖縄配送・契約B","pipeline":"743961896"}},{"id":"903","properties":{"dealname":"沖縄配送・計上","pipeline":"22417753"}}]})).into_response();
                }
                if path == "/crm/v3/objects/0-420/search" {
                    calls.lock().unwrap().push(path);
                    return Json(json!({"results":[{"id":"203","properties":{}}]})).into_response();
                }
                assert_eq!(path, "/crm/v3/objects/0-420");
                let query = req.uri().query().unwrap_or("");
                assert!(query.contains("limit=100"));
                assert!(!query.contains("hrh_kyuujinhyou_gazou"));
                if !query.contains("properties=hs_object_id") {
                    assert!(!query.contains("shigotonaiyou"));
                    assert!(!query.contains("hrh_kyuujinhyou_honbun"));
                    assert!(query.contains("kyuujin_status"));
                }
                // Like HubSpot: history reads above 50 objects are rejected with 400.
                if query.contains("propertiesWithHistory") && query.contains("limit=100") { return (StatusCode::BAD_REQUEST, Json(json!({"message":"You can only request at most 50 objects in one request for properties with history."}))).into_response(); }
                calls.lock().unwrap().push(query.into());
                if fail_next.load(Ordering::Relaxed) { return (StatusCode::FORBIDDEN, Json(json!({}))).into_response(); }
                let start = query.split('&').find_map(|p| p.strip_prefix("after="))
                    .map(|s| s.parse::<usize>().unwrap().max(1)).unwrap_or(1);
                let end = (start + 99).min(203);
                let results: Vec<Value> = (start..=end).map(|id| {
                    let (pref, title, airwork) = if id <= 150 {("沖縄県", "看護師", false)} else if id <= 180 {("沖縄県", "看護師", true)} else if id <= 200 {("大分県", "ドライバー", false)} else {("沖縄県", "清掃スタッフ", false)};
                    let mut properties = json!({"hs_name":format!("合成求人{id:03}"),"todoufuken":pref,"shikuchouson":"合成市","hrh_kyuujinhyou_honbun":format!("仕事内容：索引に残さない合成の長い本文\nIndeed表示職種名：{title}"),"zhizhong":title,"jobcopy_indeed_title":if title == "清掃スタッフ" { "" } else { title },"qinwude":format!("{pref}合成市"),"kyuujin_status":"公開中"});
                    if airwork { properties["id_airwork"] = json!(format!("AW-{id}")); properties["airwork_account_login_id"] = json!("synthetic-account"); }
                    else { properties["id_hrhakkaa"] = json!(format!("HR-{id}")); }
                    json!({"id":id.to_string(),"properties":properties})
                }).collect();
                let mut data = json!({"results":results});
                // Real 0-420 behavior verified 2026-10-15: inclusive after, last ID + 1.
                if end < 203 {data["paging"] = json!({"next":{"after":(end + 1).to_string()}});}
                Json(data).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service =
            JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let cache = service.listing_index.clone();
        assert!(cache.state.read().await.snapshot.is_none());
        refresh(
            &cache,
            &service,
            vec!["看護師".into(), "ドライバー".into()],
            Duration::ZERO,
        )
        .await
        .unwrap();
        update_draft_state(
            &service,
            "180",
            crate::job_gen::drafts::DraftStatus::Pending,
        )
        .await;
        let old = cache.state.read().await.snapshot.clone().unwrap();
        assert_eq!(old.records.len(), 203);
        assert!(old
            .records
            .iter()
            .all(|row| row.publication_status.as_deref() == Some("掲載中")));
        assert_eq!(seen.lock().unwrap().len(), 27);
        assert_eq!(
            old.records.iter().find(|r| r.id == "1").unwrap().deal_ids,
            vec!["901", "902"]
        );
        assert_eq!(old.records[0].indexed_application_count, Some(2));
        assert_eq!(
            deal_candidates(&old, "配送")
                .iter()
                .map(|d| (d.name.as_deref().unwrap(), d.count))
                .collect::<Vec<_>>(),
            vec![("沖縄配送・契約A", 2), ("沖縄配送・契約B", 1)]
        );
        assert!(deal_candidates(&old, "合わない取引").is_empty());
        let encoded = serde_json::to_string(&old.records).unwrap();
        assert!(!encoded.contains("hrh_kyuujinhyou_honbun"));
        assert!(!encoded.contains("索引に残さない"));
        assert!(!encoded.contains("application_count"));
        let parse = |v: Value| serde_json::from_value::<ListingsQuery>(v).unwrap();
        assert_eq!(
            selected_page(
                &old,
                &parse(json!({"deal":"901","prefecture":"沖縄県","title":"看護師","media":"hrh"}))
            )
            .0,
            2
        );
        assert_eq!(selected_page(&old, &parse(json!({"deal":"902"}))).0, 1);
        assert_eq!(selected_page(&old, &parse(json!({"deal":"none"}))).0, 201);
        assert_eq!(
            selected_page(&old, &parse(json!({"deal":"901","media":"airwork"}))).0,
            0
        );
        assert_eq!(
            selected_page(&old, &parse(json!({"draft_status":"pending"}))).0,
            1
        );
        assert_eq!(
            selected_page(&old, &parse(json!({"draft_status":"none"}))).0,
            202
        );
        update_draft_state(
            &service,
            "180",
            crate::job_gen::drafts::DraftStatus::Adopted,
        )
        .await;
        let updated = cache.state.read().await.snapshot.clone().unwrap();
        assert_eq!(
            selected_page(&updated, &parse(json!({"draft_status":"adopted"}))).0,
            1
        );
        assert_eq!(
            selected_page(&updated, &parse(json!({"draft_status":"pending"}))).0,
            0
        );
        assert_eq!(
            selected_page(&old, &parse(json!({"prefecture":"沖縄県"}))).0,
            183
        );
        assert_eq!(
            selected_page(&old, &parse(json!({"title":"看護師"}))).0,
            180
        );
        assert_eq!(
            selected_page(&old, &parse(json!({"media":"airwork"}))).0,
            30
        );
        assert_eq!(selected_page(&old, &parse(json!({"title":"unknown"}))).0, 3);
        let query =
            parse(json!({"prefecture":"沖縄県","title":"看護師","media":"hrh","sort":"id"}));
        let (total, first) = selected_page(&old, &query);
        assert_eq!(total, 150);
        assert_eq!(first.len(), 50);
        assert_eq!(first.first().unwrap().listing.id, "1");
        assert_eq!(first.last().unwrap().listing.id, "50");
        let (total, last) = selected_page(
            &old,
            &ListingsQuery {
                offset: 100,
                ..query
            },
        );
        assert_eq!(total, 150);
        assert_eq!(last.first().unwrap().listing.id, "101");
        assert_eq!(last.last().unwrap().listing.id, "150");
        assert!(selected_page(&old, &parse(json!({"offset":203})))
            .1
            .is_empty());
        let (_, sorted) = selected_page(&old, &parse(json!({"sort":"media"})));
        assert_eq!(sorted[0].listing.id, "151");
        assert_eq!(
            service.hs.background().priority(),
            crate::hubspot::gateway::Priority::Background
        );
        let before_failure = seen.lock().unwrap().len();
        failed.store(true, Ordering::Relaxed);
        assert!(
            refresh(&cache, &service, old.titles.clone(), Duration::ZERO)
                .await
                .is_err()
        );
        let state = cache.state.read().await;
        assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &updated));
        assert_eq!(state.snapshot.as_ref().unwrap().built_at, old.built_at);
        assert!(state.refresh_failed);
        assert!(!state.refreshing);
        assert!((1..=8).contains(&(seen.lock().unwrap().len() - before_failure)));
        task.abort();
    }
    #[test]
    fn job_copy_history_limit_nineteen_and_twenty_per_property() {
        for count in [19, 20] {
            let history: Vec<_> = (1..=count).map(|day| json!({"timestamp":format!("2026-10-{day:02}T00:00:00Z"),"value":format!("合成本文{day}")})).collect();
            let data = json!({"propertiesWithHistory":{"shigotonaiyou":history}});
            let (rows, counts, incomplete) = versions(&data, "airwork").unwrap();
            assert_eq!(rows.len(), count);
            assert_eq!(counts["shigotonaiyou"], count);
            assert_eq!(incomplete, count >= 20);
            let images: Vec<_> = (1..=count)
                .map(
                    |day| json!({"timestamp":format!("2026-10-{day:02}T00:00:00Z"),"value":"なし"}),
                )
                .collect();
            let (_, counts, incomplete) = versions(
                &json!({"propertiesWithHistory":{"hrh_kyuujinhyou_gazou":images}}),
                "hrh",
            )
            .unwrap();
            assert_eq!(counts["hrh_kyuujinhyou_gazou"], count);
            assert_eq!(incomplete, count >= 20);
        }
    }
    #[test]
    fn job_copy_current_body_is_available_without_history_and_has_a_read_timestamp() {
        let record: Record = serde_json::from_value(json!({"id":"42","properties":{"id_airwork":"AW-42","shigotonaiyou":"仕事内容：合成の看護業務\n給与：時給1800円"}})).unwrap();
        assert!(versions(&json!({}), "airwork").unwrap().0.is_empty());
        let current = current_body(&record, "airwork").unwrap();
        assert_eq!(
            current["body"],
            "仕事内容：合成の看護業務\n給与：時給1800円"
        );
        assert!(
            chrono::DateTime::parse_from_rfc3339(current["checked_at"].as_str().unwrap()).is_ok()
        );
        assert!(current["image_urls"].is_null());
        assert!(current_body(&record, "hrh").is_none());
    }
    #[tokio::test]
    async fn job_copy_observed_images_require_verified_manifest_and_customer_relation() {
        use crate::handlers::{
            job_copy_drive::DriveReader,
            job_copy_image_bridge::{ImageBridge, PROPERTY},
        };
        use axum::{body::Body, http::Request, routing::any};
        use sha2::{Digest, Sha256};
        use std::sync::atomic::{AtomicBool, Ordering};
        let related = Arc::new(AtomicBool::new(true));
        let date = "2026-10-10T00:00:00Z";
        let manifest = json!({"schemaVersion":1,"listingId":"30","companyIds":["10"],"observedAt":date,"operationId":"a".repeat(64),"images":(1..=5).rev().map(|slot| json!({"slot":slot,"fileId":format!("synthetic_image_{slot:03}"),"sha256":"b".repeat(64),"mimeType":"image/png","size":100})).collect::<Vec<_>>()});
        let raw = serde_json::to_vec(&manifest).unwrap();
        let pointer = json!({"fileId":"synthetic_manifest_001","sha256":format!("{:x}", Sha256::digest(&raw)),"observedAt":date}).to_string();
        let relation = related.clone();
        let upstream = Router::new().fallback(any(move |request: Request<Body>| {
            let raw = raw.clone(); let relation = relation.clone();
            async move {
                let path = request.uri().path();
                match path {
                    "/token" => Json(json!({"access_token":"synthetic-only","token_type":"Bearer","expires_in":3600})).into_response(),
                    "/drive/v3/files/synthetic_manifest_001" => ([(header::CONTENT_TYPE,"application/json")], raw).into_response(),
                    "/crm/v4/objects/companies/10/associations/deals" => {
                        assert_eq!(request.method(), reqwest::Method::GET);
                        Json(json!({"results":[{"toObjectId":"20"}]})).into_response()
                    }
                    "/crm/v4/objects/0-420/30/associations/deals" => Json(json!({"results":[{"toObjectId":if relation.load(Ordering::Relaxed) {"20"} else {"99"}}]})).into_response(),
                    _ => panic!("unexpected mock read path"),
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, upstream).await.unwrap();
        });
        let access = Access {
            allowed: BTreeSet::new(),
            service: None,
            moc_path: None,
            moc_drive: Ok(None),
            snapshot_reader: None,
            snapshot_cache: Arc::default(),
            resolved_jobs: Arc::default(),
            images: Some(Arc::new(ImageBridge::for_test(
                &base,
                Arc::new(DriveReader::for_test(&base)),
            ))),
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
        };
        let record = Record {
            id: "30".into(),
            properties: BTreeMap::from([(PROPERTY.into(), Some(pointer))]),
        };
        let observation = observed_images(&record, &access).await.unwrap();
        assert_eq!(observation["observed_at"], date);
        assert_eq!(observation["image_urls"][0], "/api/job-copy/image?company_id=10&listing_id=30&manifest_id=synthetic_manifest_001&slot=1");
        assert_eq!(observation["image_urls"].as_array().unwrap().len(), 5);
        assert_eq!(observation["image_urls"][4], "/api/job-copy/image?company_id=10&listing_id=30&manifest_id=synthetic_manifest_001&slot=5");
        related.store(false, Ordering::Relaxed);
        assert!(observed_images(&record, &access).await.is_none());
        assert!(observed_images(
            &Record {
                id: "31".into(),
                ..record
            },
            &access
        )
        .await
        .is_none());
        task.abort();
    }
}

#[cfg(test)]
#[path = "listing_index_loading_tests.rs"]
mod index_loading_tests;

#[path = "listing_index_summary.rs"]
mod index_summary;
use index_summary::{enrich_index_summaries, index_properties};
