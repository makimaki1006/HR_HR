//! Complete, in-memory listing index. No Search calls, stored bodies or applicant properties.
use super::*;
use axum::extract::Path as AxumPath;
use unicode_normalization::UnicodeNormalization;

const PROPERTIES: &[&str] = &[
    "hs_name",
    "id_hrhakkaa",
    "id_airwork",
    "airwork_account_login_id",
    "id_shop_hrhakkaa",
    "hrh_kyuujinhyou_honbun",
    "hrh_kyuujinhyou_gazou",
    "todoufuken",
    "shikuchouson",
    "qinwude",
    "zhizhong",
    "shigotonaiyou",
    "baitai_genjoukyou_hrhakkaa",
    "baitai_genjoukyou_airwork",
    "saishuu_csv_kenshutsu_bi",
];
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListingsQuery {
    prefecture: Option<String>,
    title: Option<String>,
    media: Option<String>,
    #[serde(default)]
    offset: usize,
    sort: Option<String>,
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
    category: Option<String>,
    publication_status: Option<String>,
    last_csv_detected_at: Option<String>,
}
fn normalize(value: &str) -> String {
    value
        .nfkc()
        .filter(|c| !c.is_whitespace() && !"・／/()（）【】".contains(*c))
        .flat_map(char::to_lowercase)
        .collect()
}
fn match_title(value: &str, titles: &[String]) -> Option<String> {
    let target = normalize(value);
    if target.is_empty() {
        return None;
    }
    if let Some(exact) = titles.iter().find(|t| normalize(t) == target) {
        return Some(exact.clone());
    }
    let mut candidates: Vec<_> = titles
        .iter()
        .filter(|t| {
            let category = normalize(t);
            category.chars().count() >= 2
                && (target.contains(&category) || category.contains(&target))
        })
        .collect();
    // Match marketMatch.ts without depending on the server/browser locale.
    candidates.sort_by(|a, b| {
        normalize(b)
            .chars()
            .count()
            .cmp(&normalize(a).chars().count())
            .then_with(|| a.cmp(b))
    });
    candidates.first().map(|t| (*t).clone())
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
fn listing(record: &Record, titles: &[String]) -> Option<Listing> {
    let hrh = record.value("id_hrhakkaa");
    let aw = record.value("id_airwork");
    let (media, media_job_id, account_id) = match (hrh, aw) {
        (Some(id), None) => ("hrh", id, record.value("id_shop_hrhakkaa")),
        (None, Some(id)) => ("airwork", id, record.value("airwork_account_login_id")),
        _ => return None,
    };
    let (prefecture, municipality, occupation) = if media == "hrh" {
        let body = record.value("hrh_kyuujinhyou_honbun").unwrap_or("");
        let occupation = super::hrh_copy::fields(body)
            .remove("Indeed表示職種名")
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .collect::<String>();
        (
            record.value("todoufuken").map(str::to_owned),
            record.value("shikuchouson").map(str::to_owned),
            occupation,
        )
    } else {
        let (pref, city) = location(record.value("qinwude").unwrap_or(""));
        (
            pref,
            city,
            record.value("zhizhong").unwrap_or("").to_owned(),
        )
    };
    Some(Listing {
        id: record.id.clone(),
        media,
        media_job_id: media_job_id.into(),
        account_id: account_id.map(str::to_owned),
        title: record.value("hs_name").map(str::to_owned),
        prefecture,
        municipality,
        category: match_title(&occupation, titles),
        publication_status: record
            .value(if media == "hrh" {
                "baitai_genjoukyou_hrhakkaa"
            } else {
                "baitai_genjoukyou_airwork"
            })
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
fn service(access: &Access) -> Result<&JobReadService, ReadError> {
    access.service.as_deref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))
}
const INDEX_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const INDEX_RETRY: Duration = Duration::from_secs(10 * 60);
const PAGE_PAUSE: Duration = Duration::from_secs(1);
const PAGE_SIZE: usize = 50;

#[derive(Default)]
pub(super) struct IndexCache {
    state: tokio::sync::RwLock<IndexState>,
    worker: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
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
}
struct Index {
    records: Vec<Listing>,
    titles: Vec<String>,
    built_at: String,
}
#[derive(Serialize)]
struct PageListing {
    #[serde(flatten)]
    listing: Listing,
    application_count: Option<usize>,
}

/// One complete build. A page's body exists only until its title line is classified.
async fn build_index(
    service: &JobReadService,
    titles: Vec<String>,
    pause: Duration,
) -> Result<Index, ReadError> {
    let background = service.hs.background();
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    let mut cursors = BTreeSet::new();
    let mut after: Option<String> = None;
    let properties = PROPERTIES
        .iter()
        .copied()
        .filter(|p| !["hrh_kyuujinhyou_gazou", "shigotonaiyou"].contains(p))
        .collect::<Vec<_>>()
        .join(",");
    loop {
        let mut params = vec![("limit", "100".into()), ("properties", properties.clone())];
        if let Some(cursor) = &after {
            params.push(("after", cursor.clone()));
        }
        let data = service
            .request_with_client(&background, "/crm/v3/objects/0-420", &params, None)
            .await?;
        let page: Page =
            serde_json::from_value(data).map_err(|_| fail("hubspot_invalid_response"))?;
        if page.results.len() > 100 {
            return Err(fail("hubspot_invalid_response"));
        }
        for record in page.results {
            valid_id(&record.id).map_err(|_| fail("hubspot_invalid_response"))?;
            // Stable IDs can repeat if a listing moves between pages during the read.
            if ids.insert(record.id.clone()) {
                if let Some(row) = listing(&record, &titles) {
                    records.push(row);
                }
            }
        }
        after = page.paging.and_then(|p| p.next).map(|n| n.after);
        match &after {
            None => break,
            Some(cursor) => {
                valid_id(cursor).map_err(|_| fail("listing_paging_invalid"))?;
                if !cursors.insert(cursor.clone()) {
                    return Err(fail("listing_paging_invalid"));
                }
                tokio::time::sleep(pause).await;
            }
        }
    }
    records.shrink_to_fit();
    Ok(Index {
        records,
        titles,
        built_at: chrono::Utc::now().to_rfc3339(),
    })
}
pub(super) async fn refresh(
    cache: &Cache,
    service: &JobReadService,
    titles: Vec<String>,
    pause: Duration,
) -> Result<(), ReadError> {
    cache.state.write().await.refreshing = true;
    let result = build_index(service, titles, pause).await;
    let mut state = cache.state.write().await;
    state.refreshing = false;
    state.refresh_failed = result.is_err();
    match result {
        Ok(index) => {
            state.snapshot = Some(Arc::new(index));
            Ok(())
        }
        Err(error) => Err(error), // Atomic replacement: an incomplete build never removes the old index.
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
                    cache.state.write().await.refresh_failed = true;
                    Err(error)
                }
            };
            if let Err(error) = &result {
                tracing::warn!(code = error.code(), "job-copy listing index refresh failed");
            }
            let delay = if result.is_ok() {
                INDEX_TTL
            } else {
                INDEX_RETRY
            };
            drop(cache);
            tokio::time::sleep(delay).await;
        }
    }));
}
fn matches(row: &Listing, query: &ListingsQuery) -> bool {
    query.media.as_ref().is_none_or(|v| v == row.media)
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
fn selected_page(index: &Index, query: &ListingsQuery) -> (usize, Vec<PageListing>) {
    let mut rows: Vec<_> = index
        .records
        .iter()
        .filter(|row| matches(row, query))
        .collect();
    rows.sort_by(|a, b| {
        let id_order = || a.id.len().cmp(&b.id.len()).then_with(|| a.id.cmp(&b.id));
        if query.sort.as_deref() == Some("id") {
            id_order()
        } else if query.sort.as_deref() == Some("media") {
            a.media
                .cmp(b.media)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(id_order)
        } else {
            a.title.cmp(&b.title).then_with(id_order)
        }
    });
    let total = rows.len();
    let page = rows
        .into_iter()
        .skip(query.offset)
        .take(PAGE_SIZE)
        .map(|row| PageListing {
            listing: row.clone(),
            application_count: None,
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
    drop(cached);
    let Some(index) = snapshot else {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(json!({
                "status":"preparing", "listings":[], "total":null, "index_built_at":null,
                "offset":query.offset, "next_offset":null, "titles":[], "refreshing":refreshing,
                "refresh_failed":refresh_failed
            })),
        ));
    };
    if query
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
        || query
            .sort
            .as_deref()
            .is_some_and(|v| !["title", "id", "media"].contains(&v))
    {
        return Err(ReadError(StatusCode::BAD_REQUEST, "listing_filter_invalid"));
    }
    let (total, mut rows) = selected_page(&index, &query);
    if !rows.is_empty() {
        let counts = service.request("/crm/v4/associations/0-420/0-421/batch/read", &[],
            Some(json!({"inputs":rows.iter().map(|row| json!({"id":row.listing.id})).collect::<Vec<_>>()}))).await;
        if let Ok(data) = counts {
            for row in &mut rows {
                row.application_count = association_count(&data, &row.listing.id);
            }
        }
    }
    let next_offset = query
        .offset
        .checked_add(rows.len())
        .filter(|next| *next < total);
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "status":"ready", "listings":rows, "total":total, "index_built_at":index.built_at,
            "offset":query.offset, "next_offset":next_offset, "titles":index.titles,
            "refreshing":refreshing, "refresh_failed":refresh_failed
        })),
    ))
}
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
}
#[derive(Deserialize)]
struct History {
    timestamp: String,
    value: String,
}
fn image_urls(value: &str) -> Vec<String> {
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
        vec!["shigotonaiyou"]
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
    let mut result: Vec<Version> = Vec::new();
    for (time, changes) in events {
        if let Some(value) = changes.get(body_property) {
            body = Some(value.clone());
        }
        if let Some(value) = changes.get("hrh_kyuujinhyou_gazou") {
            images = Some(image_urls(value));
        }
        if let Some(body) = &body {
            if result
                .last()
                .is_some_and(|last| &last.body == body && last.image_urls == images)
            {
                continue;
            }
            result.push(Version {
                written_at: time.to_rfc3339(),
                body: body.clone(),
                image_urls: images.clone(),
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
    let images = if media == "hrh" {
        record.value("hrh_kyuujinhyou_gazou").map(image_urls)
    } else {
        None
    };
    Some(json!({"checked_at":chrono::Utc::now().to_rfc3339(),"body":body,"image_urls":images}))
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
pub(super) async fn read_versions(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let data = service(&access)?
        .request(
            &format!("/crm/v3/objects/0-420/{id}"),
            &[
                (
                    "properties",
                    format!(
                        "{},{}",
                        PROPERTIES.join(","),
                        super::super::job_copy_image_bridge::PROPERTY
                    ),
                ),
                (
                    "propertiesWithHistory",
                    "hrh_kyuujinhyou_honbun,hrh_kyuujinhyou_gazou,shigotonaiyou".into(),
                ),
            ],
            None,
        )
        .await?;
    let record: Record =
        serde_json::from_value(data.clone()).map_err(|_| fail("hubspot_invalid_response"))?;
    if record.id != id {
        return Err(fail("hubspot_invalid_response"));
    }
    let db = state.indeed_db.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "market_data_unavailable",
    ))?;
    let row = listing(&record, &taxonomy(db)?).ok_or(ReadError(
        StatusCode::UNPROCESSABLE_ENTITY,
        "listing_media_unknown",
    ))?;
    let (versions, counts, may_be_incomplete) = versions(&data, row.media)?;
    let current = if versions
        .iter()
        .all(|version| version.body.trim().is_empty())
    {
        current_body(&record, row.media)
    } else {
        None
    };
    let current_images = observed_images(&record, &access).await;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(
            json!({"listing":PageListing {listing: row, application_count: None},"versions":versions,"history_counts":counts,"history_may_be_incomplete":may_be_incomplete,"current":current,"current_images":current_images}),
        ),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn job_copy_multiline_occupation_is_classified_and_filtered() {
        let record: Record = serde_json::from_value(json!({"id":"1","properties":{"id_hrhakkaa":"HR-1","hrh_kyuujinhyou_honbun":"仕事内容：配送します\nIndeed表示職種名：\n配送\nドライバー\n応募資格：普通免許","todoufuken":"沖縄県"}})).unwrap();
        let titles = vec!["配送ドライバー".into()];
        let row = listing(&record, &titles).unwrap();
        assert_eq!(row.category.as_deref(), Some("配送ドライバー"));
        let index = Index {
            records: vec![row],
            titles,
            built_at: "2026-10-10T00:00:00Z".into(),
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
    fn job_copy_title_and_prefecture_specific_values() {
        let titles = vec![
            "ドライバー".into(),
            "配送ドライバー".into(),
            "看護師".into(),
        ];
        assert_eq!(
            match_title("配送 ドライバー", &titles),
            Some("配送ドライバー".into())
        );
        assert_eq!(
            match_title("夜間配送ドライバー募集", &titles),
            Some("配送ドライバー".into())
        );
        assert_eq!(match_title("運転手", &titles), None);
        assert_eq!(
            match_title("看護師・介護職", &["看護師".into(), "介護職".into()]),
            Some("介護職".into())
        );
        assert_eq!(match_title("", &titles), None);
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
    fn job_copy_media_keys_and_unknown_values() {
        let titles = vec!["ドライバー".into()];
        let make = |id: &str, account: &str| {
            serde_json::from_value::<Record>(json!({"id":id,"properties":{"id_airwork":"same-id","airwork_account_login_id":account,"qinwude":"千葉県市川市合成勤務地","zhizhong":"運転手"}})).unwrap()
        };
        let first = listing(&make("10", "synthetic-a"), &titles).unwrap();
        let second = listing(&make("11", "synthetic-b"), &titles).unwrap();
        assert_eq!(first.media_job_id, second.media_job_id);
        assert_ne!(first.account_id, second.account_id);
        assert_eq!(first.prefecture, Some("千葉県".into()));
        assert_eq!(first.municipality, Some("市川市".into()));
        assert_eq!(first.category, None);
        assert_eq!(first.publication_status, None);
        let mut published = make("12", "synthetic-c");
        published
            .properties
            .insert("baitai_genjoukyou_airwork".into(), Some("掲載中".into()));
        assert_eq!(
            listing(&published, &titles).unwrap().publication_status,
            Some("掲載中".into())
        );

        assert_eq!(image_urls("画像1：https://example.invalid/img,a.png 画像2：なし 画像3：https://example.invalid/3.png"), vec!["https://example.invalid/img,a.png", "https://example.invalid/3.png"]);
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
                assert_eq!(req.uri().path(), "/crm/v3/objects/0-420");
                let query = req.uri().query().unwrap_or("");
                assert!(query.contains("limit=100"));
                assert!(!query.contains("hrh_kyuujinhyou_gazou"));
                assert!(!query.contains("shigotonaiyou"));
                calls.lock().unwrap().push(query.into());
                if fail_next.load(Ordering::Relaxed) { return (StatusCode::FORBIDDEN, Json(json!({}))).into_response(); }
                let start = if query.contains("after=200") {201} else if query.contains("after=100") {101} else {1};
                let end = (start + 99).min(203);
                let results: Vec<Value> = (start..=end).map(|id| {
                    let (pref, title, airwork) = if id <= 150 {("沖縄県", "看護師", false)} else if id <= 180 {("沖縄県", "看護師", true)} else if id <= 200 {("大分県", "ドライバー", false)} else {("沖縄県", "清掃スタッフ", false)};
                    let mut properties = json!({"hs_name":format!("合成求人{id:03}"),"todoufuken":pref,"shikuchouson":"合成市","hrh_kyuujinhyou_honbun":format!("Indeed表示職種名：{title}\n仕事内容：索引に残さない合成の長い本文"),"zhizhong":title,"qinwude":format!("{pref}合成市")});
                    if airwork { properties["id_airwork"] = json!(format!("AW-{id}")); properties["airwork_account_login_id"] = json!("synthetic-account"); }
                    else { properties["id_hrhakkaa"] = json!(format!("HR-{id}")); }
                    json!({"id":id.to_string(),"properties":properties})
                }).collect();
                let mut data = json!({"results":results});
                if end < 203 {data["paging"] = json!({"next":{"after":end.to_string()}});}
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
        let old = cache.state.read().await.snapshot.clone().unwrap();
        assert_eq!(old.records.len(), 203);
        assert_eq!(seen.lock().unwrap().len(), 3);
        let encoded = serde_json::to_string(&old.records).unwrap();
        assert!(!encoded.contains("hrh_kyuujinhyou_honbun"));
        assert!(!encoded.contains("索引に残さない"));
        assert!(!encoded.contains("application_count"));
        let parse = |v: Value| serde_json::from_value::<ListingsQuery>(v).unwrap();
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
        failed.store(true, Ordering::Relaxed);
        assert!(
            refresh(&cache, &service, old.titles.clone(), Duration::ZERO)
                .await
                .is_err()
        );
        let state = cache.state.read().await;
        assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &old));
        assert_eq!(state.snapshot.as_ref().unwrap().built_at, old.built_at);
        assert!(state.refresh_failed);
        assert!(!state.refreshing);
        assert_eq!(seen.lock().unwrap().len(), 4);
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
        let manifest = json!({"schemaVersion":1,"listingId":"30","companyIds":["10"],"observedAt":date,"operationId":"a".repeat(64),"images":[{"slot":1,"fileId":"synthetic_image_001","sha256":"b".repeat(64),"mimeType":"image/png","size":100}]});
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
