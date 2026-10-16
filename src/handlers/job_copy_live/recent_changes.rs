//! 更新候補だけの履歴を背景で取得。本文は集計後に捨て、求人ごとの結果だけを保持する。
use super::*;
use chrono::{DateTime, NaiveDate, Utc};
use std::time::Instant;

const PROPERTIES: &[&str] = &[
    "hrh_kyuujinhyou_honbun",
    "shigotonaiyou",
    "kyuuyokeitai",
    "hs_name",
];
// 本番読み取りで100件は400、50件は200を確認。履歴付きの要求は50件に抑える。
const HISTORY_BATCH: usize = 50;
const TTL: Duration = Duration::from_secs(3 * 60 * 60);
const RETRY: Duration = Duration::from_secs(60);
#[derive(Default)]
pub(super) struct Cache {
    state: tokio::sync::Mutex<CacheState>,
}
#[derive(Default)]
struct CacheState {
    snapshot: Option<(NaiveDate, Instant, Arc<Report>)>,
    refreshing: bool,
    failed_at: Option<Instant>,
}
#[derive(Debug, Serialize)]
struct ChangedDeal {
    id: String,
    name: Option<String>,
    company: Option<String>,
}
#[derive(Debug, Serialize)]
struct ChangedListing {
    id: String,
    title: Option<String>,
    media: Option<String>,
    publication_status: Option<String>,
    deals: Vec<ChangedDeal>,
    fields: BTreeSet<&'static str>,
    last_changed_at: DateTime<Utc>,
}
#[derive(Debug, Serialize)]
struct Report {
    count: usize,
    listings: Vec<ChangedListing>,
}
fn bounds(today: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let midnight = today.and_hms_opt(0, 0, 0).unwrap().and_utc() - chrono::Duration::hours(9);
    (
        midnight - chrono::Duration::days(6),
        midnight + chrono::Duration::days(1),
    )
}

/// 不正な履歴・同時刻の異なる値・現在値との不一致は判定不能。
/// 隣り合う異なる値の組だけを見るため、最初の記録や同値の再保存は数えない。
fn changes<T: PartialEq>(
    data: &Value,
    key: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: impl Fn(&str) -> T,
) -> Option<DateTime<Utc>> {
    let mut history: Vec<_> = data["propertiesWithHistory"][key]
        .as_array()?
        .iter()
        .map(|v| {
            Some((
                DateTime::parse_from_rfc3339(v["timestamp"].as_str()?)
                    .ok()?
                    .with_timezone(&Utc),
                v["value"].as_str()?,
            ))
        })
        .collect::<Option<_>>()?;
    history.sort_by_key(|entry| entry.0);
    if history
        .windows(2)
        .any(|w| w[0].0 == w[1].0 && w[0].1 != w[1].1)
    {
        return None;
    }
    if history.last()?.1 != data["properties"][key].as_str()? {
        return None;
    }
    history
        .windows(2)
        .filter(|w| w[1].0 >= start && w[1].0 < end && project(w[0].1) != project(w[1].1))
        .map(|w| w[1].0)
        .max()
}
fn salary(body: &str, hrh: bool) -> Vec<Option<String>> {
    let fields = hrh_copy::fields(body);
    let keys: &[&str] = if hrh {
        &["基本給与 最小", "基本給与 最大", "給与形態"]
    } else {
        &["基本給与 最小", "基本給与 最大"]
    };
    keys.iter()
        .map(|key| fields.get(key).filter(|v| !v.is_empty()).cloned())
        .collect()
}
fn detect(
    data: &Value,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> BTreeMap<&'static str, DateTime<Utc>> {
    let mut found = BTreeMap::new();
    for (key, label) in [
        ("hrh_kyuujinhyou_honbun", "本文"),
        ("shigotonaiyou", "本文"),
        ("hs_name", "求人名"),
        ("kyuuyokeitai", "給与"),
    ] {
        if let Some(at) = changes(data, key, start, end, str::to_owned) {
            found
                .entry(label)
                .and_modify(|old: &mut DateTime<Utc>| *old = (*old).max(at))
                .or_insert(at);
        }
    }
    for (key, hrh) in [("hrh_kyuujinhyou_honbun", true), ("shigotonaiyou", false)] {
        if let Some(at) = changes(data, key, start, end, |body| salary(body, hrh)) {
            found
                .entry("給与")
                .and_modify(|old| *old = (*old).max(at))
                .or_insert(at);
        }
    }
    found
}

#[derive(Default)]
struct Stats {
    candidates: usize,
    search_calls: usize,
    history_calls: usize,
}
/// 1万件以上の区間は二分割。ページを取得する前に total を確認する。
async fn candidate_ids(
    service: &JobReadService,
    start: i64,
    end: i64,
    stats: &mut Stats,
) -> Result<Vec<String>, ReadError> {
    let mut ranges = vec![(start, end)];
    let mut ids = BTreeSet::new();
    while let Some((low, high)) = ranges.pop() {
        let mut after: Option<Value> = None;
        loop {
            let mut body = json!({"filterGroups":[{"filters":[
                {"propertyName":"hs_lastmodifieddate","operator":"GTE","value":low.to_string()},
                {"propertyName":"hs_lastmodifieddate","operator":"LT","value":high.to_string()}
            ]}],"properties":["hs_object_id"],"limit":200,"sorts":["hs_lastmodifieddate"]});
            if let Some(cursor) = &after {
                body["after"] = cursor.clone();
            }
            let data = service
                .request("/crm/v3/objects/0-420/search", &[], Some(body))
                .await?;
            stats.search_calls += 1;
            let total = data["total"]
                .as_u64()
                .ok_or_else(|| fail("recent_changes_invalid_search"))?;
            if total >= 10_000 {
                if high - low <= 1 {
                    return Err(fail("recent_changes_search_limit"));
                }
                let mid = low + (high - low) / 2;
                ranges.extend([(low, mid), (mid, high)]);
                break;
            }
            for row in data["results"]
                .as_array()
                .ok_or_else(|| fail("recent_changes_invalid_search"))?
            {
                ids.insert(
                    row["id"]
                        .as_str()
                        .ok_or_else(|| fail("recent_changes_invalid_id"))?
                        .to_owned(),
                );
            }
            let next = data.pointer("/paging/next/after").cloned();
            if next.is_none() {
                break;
            }
            if next == after {
                return Err(fail("recent_changes_invalid_paging"));
            }
            after = next;
        }
    }
    stats.candidates = ids.len();
    Ok(ids.into_iter().collect())
}
async fn histories(
    service: &JobReadService,
    ids: &[String],
    start: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    stats: &mut Stats,
) -> Result<BTreeMap<String, BTreeMap<&'static str, DateTime<Utc>>>, ReadError> {
    let mut changed = BTreeMap::new();
    // Background の要求だけを最大4本。本文・履歴を全件分メモリに積まない。
    for group in ids.chunks(HISTORY_BATCH * 4) {
        let mut tasks = tokio::task::JoinSet::new();
        for chunk in group.chunks(HISTORY_BATCH) {
            let service = service.clone();
            let chunk = chunk.to_vec();
            tasks.spawn(async move { history_batch(&service, &chunk, start, cutoff).await });
        }
        while let Some(result) = tasks.join_next().await {
            let rows = result.map_err(|_| fail("recent_changes_worker_failed"))??;
            stats.history_calls += 1;
            changed.extend(rows);
        }
    }
    Ok(changed)
}
async fn history_batch(
    service: &JobReadService,
    chunk: &[String],
    start: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> Result<BTreeMap<String, BTreeMap<&'static str, DateTime<Utc>>>, ReadError> {
    let mut changed = BTreeMap::new();
    let data = service
        .request(
            "/crm/v3/objects/0-420/batch/read",
            &[],
            Some(json!({
                "inputs":chunk.iter().map(|id| json!({"id":id})).collect::<Vec<_>>(),
                "properties":PROPERTIES,"propertiesWithHistory":PROPERTIES
            })),
        )
        .await?;
    let rows = data["results"]
        .as_array()
        .ok_or_else(|| fail("recent_changes_invalid_history"))?;
    let received: BTreeSet<_> = rows.iter().filter_map(|row| row["id"].as_str()).collect();
    if data["errors"].as_array().is_some_and(|e| !e.is_empty())
        || rows.len() != chunk.len()
        || received.len() != chunk.len()
        || chunk.iter().any(|id| !received.contains(id.as_str()))
    {
        return Err(fail("recent_changes_incomplete_history"));
    }
    for row in rows {
        let fields = detect(row, start, cutoff);
        if !fields.is_empty() {
            changed.insert(row["id"].as_str().unwrap().to_owned(), fields);
        }
    }
    Ok(changed)
}

async fn fetch(service: &JobReadService, today: NaiveDate) -> Result<Report, ReadError> {
    let (start, end) = bounds(today);
    let mut stats = Stats::default();
    // 現時刻まで。翌日の値や Search 中の更新による窓の移動を避ける。
    let cutoff = Utc::now().min(end);
    let ids = candidate_ids(
        service,
        start.timestamp_millis(),
        cutoff.timestamp_millis(),
        &mut stats,
    )
    .await?;
    let changed = histories(service, &ids, start, cutoff, &mut stats).await?;
    tracing::info!(
        candidates = stats.candidates,
        search_calls = stats.search_calls,
        history_calls = stats.history_calls,
        "job-copy recent changes reads"
    );
    resolve(service, &changed).await
}

async fn resolve(
    service: &JobReadService,
    changed: &BTreeMap<String, BTreeMap<&'static str, DateTime<Utc>>>,
) -> Result<Report, ReadError> {
    let ids: Vec<_> = changed.keys().cloned().collect();
    let associations = service.association_map("0-420", &ids, "deals").await?;
    let deal_ids: Vec<_> = associations
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let deals = service
        .batch("deals", &deal_ids, &["dealname", "pipeline"])
        .await?;
    let mut report = aggregate(changed, &associations, &deals);
    let ids: Vec<_> = report
        .listings
        .iter()
        .flat_map(|r| &r.deals)
        .map(|d| d.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let companies = service.association_map("deals", &ids, "companies").await?;
    let ids: Vec<_> = companies
        .values()
        .filter_map(|ids| match ids.as_slice() {
            [id] => Some(id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let names: BTreeMap<_, _> = service
        .batch("companies", &ids, &["name"])
        .await?
        .into_iter()
        .map(|r| (r.id.clone(), r.value("name").map(str::to_owned)))
        .collect();
    for deal in report.listings.iter_mut().flat_map(|r| &mut r.deals) {
        deal.company = companies
            .get(&deal.id)
            .and_then(|ids| match ids.as_slice() {
                [id] => names.get(id).cloned().flatten(),
                _ => None,
            });
    }
    let changed_ids = changed.keys().map(String::as_str).collect();
    let details = listings::recent_change_details(service, &changed_ids).await;
    for row in &mut report.listings {
        if let Some(detail) = details.get(&row.id) {
            row.title = detail.title.clone();
            row.media = detail.media.clone();
            row.publication_status = detail.publication_status.clone();
        }
    }
    Ok(report)
}
pub(super) struct ListingDetails {
    pub title: Option<String>,
    pub media: Option<String>,
    pub publication_status: Option<String>,
}
fn aggregate(
    changed: &BTreeMap<String, BTreeMap<&'static str, DateTime<Utc>>>,
    associations: &BTreeMap<String, Vec<String>>,
    deals: &[Record],
) -> Report {
    let mut result = Vec::new();
    for (id, fields) in changed {
        let related: Vec<_> = deals
            .iter()
            .filter(|deal| {
                listings::is_delivery(deal.value("pipeline"))
                    && associations
                        .get(id)
                        .is_some_and(|ids| ids.contains(&deal.id))
            })
            .map(|deal| ChangedDeal {
                id: deal.id.clone(),
                name: deal.value("dealname").map(str::to_owned),
                company: None,
            })
            .collect();
        if let Some(last) = fields.values().max().filter(|_| !related.is_empty()) {
            result.push(ChangedListing {
                id: id.clone(),
                title: None,
                media: None,
                publication_status: None,
                deals: related,
                fields: fields.keys().copied().collect(),
                last_changed_at: *last,
            });
        }
    }
    result.sort_by(|a, b| {
        b.last_changed_at
            .cmp(&a.last_changed_at)
            .then(a.id.cmp(&b.id))
    });
    Report {
        count: result.len(),
        listings: result,
    }
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<owner_todo::OwnerQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    query.validate()?;
    let service = listings::service(&access)?;
    let today = (Utc::now() + chrono::Duration::hours(9)).date_naive();
    let mut data = answer(service, today).await;
    if query.owner.is_some() || query.owner_field.is_some() {
        let view = owner_todo::view(service).await;
        if let Some(snapshot) = view.snapshot {
            let (owner, _) = snapshot.selection(&query, &email);
            if let Some(rows) = data["report"]["listings"].as_array_mut() {
                rows.retain(|r| {
                    r["deals"].as_array().is_some_and(|ds| {
                        ds.iter().any(|d| {
                            d["id"].as_str().is_some_and(|id| {
                                snapshot.matches(id, query.field(), owner.as_deref())
                            })
                        })
                    })
                });
                data["report"]["count"] = json!(rows.len());
            }
        } else {
            data = json!({"status":if view.failed {"failed"} else {"preparing"}});
        }
    }
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(data)))
}

async fn answer(service: &JobReadService, today: NaiveDate) -> Value {
    let mut cache = service.recent_changes.state.lock().await;
    let fresh = cache
        .snapshot
        .as_ref()
        .is_some_and(|(day, at, _)| *day == today && at.elapsed() < TTL);
    if !fresh && !cache.refreshing && !cache.failed_at.is_some_and(|at| at.elapsed() < RETRY) {
        cache.refreshing = true;
        let mut service = service.clone();
        service.hs = service.hs.background();
        tokio::spawn(async move {
            let result = fetch(&service, today).await;
            let mut cache = service.recent_changes.state.lock().await;
            match result {
                Ok(report) => {
                    cache.snapshot = Some((today, Instant::now(), Arc::new(report)));
                    cache.failed_at = None;
                }
                Err(_) => {
                    cache.failed_at = Some(Instant::now());
                    tracing::warn!("job-copy recent changes refresh failed");
                }
            }
            cache.refreshing = false;
        });
    }
    // 日付が変わったら前日の集計を今日の件数として返さない。
    if let Some((day, _, report)) = &cache.snapshot {
        if *day == today {
            json!({"status":"ready", "report":report, "refresh_failed":cache.failed_at.is_some()})
        } else {
            json!({"status":if cache.failed_at.is_some() {"failed"} else {"preparing"}})
        }
    } else {
        json!({"status":if cache.failed_at.is_some() {"failed"} else {"preparing"}})
    }
}

#[cfg(test)]
#[path = "recent_changes_tests.rs"]
mod tests;
