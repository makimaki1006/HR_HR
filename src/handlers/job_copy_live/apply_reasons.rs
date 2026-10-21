//! 応募理由 × 地域 × 職種の横断集計。応募者の情報は保存せず、プロセス内に6時間だけ持つ。
use super::*;
use chrono::{DateTime, Datelike, Months, NaiveDate, Utc};
use std::time::Instant;

const TTL: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY: Duration = Duration::from_secs(60);
const MAX_PAGES: usize = 1000;
const MONTHS: [u32; 3] = [1, 3, 6];
const TEXT_PROPERTIES: [&str; 3] = [
    "oubodouki",
    "kinmujoukennoyuusenjuni",
    "honnin_kibou_kinyuuran",
];
const APP_PROPERTIES: [&str; 10] = [
    "yingmuri",
    "hs_createdate",
    "todoufuken",
    "shikuchouson",
    "oubosaki_shokushu",
    "oubodouki",
    "kinmujoukennoyuusenjuni",
    "honnin_kibou_kinyuuran",
    "ouboriyuukategori_hiaringu",
    "ouboriyuukategori_baitaikisai",
];
const OCCUPATION_GROUPS: [&str; 11] = [
    "営業",
    "エンジニア",
    "事務",
    "技能職",
    "サービス",
    "介護",
    "医療",
    "販売",
    "物流",
    "製造",
    "その他",
];

#[derive(Default)]
pub(super) struct Cache {
    state: tokio::sync::Mutex<CacheState>,
}
#[derive(Default)]
struct CacheState {
    snapshot: Option<(NaiveDate, Instant, Arc<Report>)>,
    built_at: Option<DateTime<Utc>>,
    refreshing: bool,
    failed_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Area {
    prefecture: Option<String>,
    municipality: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Selection {
    source_property: String,
    label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Text {
    source_property: String,
    text: String,
}
#[derive(Debug, Clone, Serialize)]
struct Row {
    date: String,
    applicant_area: Area,
    job_area: Option<Area>,
    occupation: Option<String>,
    occupation_status: &'static str,
    selections: Vec<Selection>,
    texts: Vec<Text>,
}
#[derive(Debug, Clone, Serialize)]
struct Period {
    months: u32,
    since: String,
    total: usize,
    with_reason: usize,
}
#[derive(Debug, Clone, Serialize)]
struct Report {
    today: String,
    periods: Vec<Period>,
    occupation_groups: Vec<&'static str>,
    rows: Vec<Row>,
}

// ---- 職種の対応表 ----
#[derive(Deserialize)]
struct GroupTable {
    #[serde(rename = "区分")]
    groups: Vec<GroupDef>,
}
#[derive(Deserialize)]
struct GroupDef {
    #[serde(rename = "名前")]
    name: String,
    #[serde(rename = "キーワード")]
    keywords: Vec<String>,
}
fn table() -> &'static [(String, Vec<String>)] {
    static TABLE: std::sync::OnceLock<Vec<(String, Vec<String>)>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let parsed: GroupTable = serde_json::from_str(include_str!("occupation_groups.json"))
            .expect("occupation_groups.json is valid");
        parsed
            .groups
            .into_iter()
            .map(|g| {
                (
                    g.name,
                    g.keywords.iter().map(|k| k.to_lowercase()).collect(),
                )
            })
            .collect()
    })
}
/// 職種の文字から区分。当たらなければ None。
fn classify(text: &str) -> Option<&'static str> {
    let lower = text.trim().to_lowercase();
    let lower = lower.strip_prefix("その他").unwrap_or(&lower);
    table()
        .iter()
        .find(|(_, words)| words.iter().any(|w| lower.contains(w.as_str())))
        .and_then(|(name, _)| OCCUPATION_GROUPS.iter().copied().find(|g| g == name))
}

// ---- 地域 ----
fn area(prefecture: Option<&str>, municipality: Option<&str>) -> Area {
    let rounded = applicant_area::round_area(prefecture, municipality);
    let municipality = rounded.municipality.map(|m| {
        // 政令指定都市の区は市にまとめる（東京23区はそのまま）。
        match (m.find('市'), m.ends_with('区')) {
            (Some(at), true) => m[..at + '市'.len_utf8()].to_owned(),
            _ => m,
        }
    });
    Area {
        prefecture: rounded.prefecture,
        municipality,
    }
}

// ---- 応募 ----
struct App {
    id: String,
    date: NaiveDate,
    area: Area,
    occupation_text: Option<String>,
    selections: Vec<(String, String)>,
    texts: Vec<Text>,
    listing_ids: BTreeSet<String>,
}
impl App {
    fn has_reason(&self) -> bool {
        !self.selections.is_empty() || !self.texts.is_empty()
    }
}
fn nonblank(properties: &Value, key: &str) -> Option<String> {
    properties[key]
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}
fn app_date(properties: &Value) -> Option<NaiveDate> {
    if let Some(day) = nonblank(properties, "yingmuri") {
        if let Ok(d) = NaiveDate::parse_from_str(day.get(..10).unwrap_or(&day), "%Y-%m-%d") {
            return Some(d);
        }
    }
    let created = nonblank(properties, "hs_createdate")?;
    let at = DateTime::parse_from_rfc3339(&created)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            created
                .parse::<i64>()
                .ok()
                .and_then(DateTime::<Utc>::from_timestamp_millis)
        })?;
    Some((at + chrono::Duration::hours(9)).date_naive())
}
fn is_unset(value: &str, label: Option<&str>) -> bool {
    let plain = |s: &str| s.trim().to_lowercase();
    plain(value) == "unset"
        || plain(value) == "未設定"
        || label.is_some_and(|l| plain(l) == "未設定")
}
fn parse_app(row: &Value, labels: Option<&applicant_reasons::OptionLabels>) -> Option<App> {
    let id = row["id"].as_str()?.to_owned();
    let p = &row["properties"];
    let date = app_date(p)?;
    let mut selections = Vec::new();
    for property in applicant_reasons::CATEGORY_PROPERTIES {
        let Some(raw) = nonblank(p, property) else {
            continue;
        };
        let mut chosen: Vec<&str> = raw
            .split(';')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .collect();
        chosen.sort();
        chosen.dedup();
        for value in chosen {
            let label = labels
                .and_then(|l| l.get(property))
                .and_then(|o| o.get(value))
                .map(String::as_str);
            if is_unset(value, label) {
                continue;
            }
            let shown: String = applicant_reasons::mask_personal_details(label.unwrap_or(value))
                .chars()
                .take(applicant_reasons::MAX_VALUE_CHARS)
                .collect();
            selections.push((property.to_owned(), shown));
        }
    }
    let texts = TEXT_PROPERTIES
        .iter()
        .filter_map(|property| {
            let raw = nonblank(p, property)?;
            Some(Text {
                source_property: (*property).to_owned(),
                text: applicant_reasons::mask_personal_details(&raw)
                    .chars()
                    .take(applicant_reasons::MAX_TEXT_CHARS)
                    .collect(),
            })
        })
        .collect();
    let mut listing_ids = BTreeSet::new();
    for group in row["associations"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
    {
        for result in group["results"].as_array().into_iter().flatten() {
            if let Some(id) = result["id"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| result["id"].as_u64().map(|n| n.to_string()))
            {
                listing_ids.insert(id);
            }
        }
    }
    Some(App {
        id,
        date,
        area: area(p["todoufuken"].as_str(), p["shikuchouson"].as_str()),
        occupation_text: nonblank(p, "oubosaki_shokushu"),
        selections,
        texts,
        listing_ids,
    })
}

// ---- 読み取り ----
async fn read_labels(service: &JobReadService) -> Option<applicant_reasons::OptionLabels> {
    let data = service
        .request(
            "/crm/v3/properties/0-421/batch/read",
            &[],
            Some(json!({"archived":false,"inputs":applicant_reasons::CATEGORY_PROPERTIES.iter().map(|name|json!({"name":name})).collect::<Vec<_>>()})),
        )
        .await
        .ok()?;
    option_labels(&data)
}
#[derive(Default)]
struct Stats {
    pages: usize,
    listing_calls: usize,
}
async fn read_apps(
    service: &JobReadService,
    since: NaiveDate,
    today: NaiveDate,
    labels: Option<&applicant_reasons::OptionLabels>,
    stats: &mut Stats,
) -> Result<(Vec<NaiveDate>, Vec<App>), ReadError> {
    let mut dates = Vec::new();
    let mut apps = Vec::new();
    let mut after: Option<String> = None;
    let mut seen = BTreeSet::new();
    for _ in 0..MAX_PAGES {
        let mut query = vec![
            ("limit", "100".to_owned()),
            ("properties", APP_PROPERTIES.join(",")),
            ("associations", "0-420".to_owned()),
        ];
        if let Some(cursor) = &after {
            query.push(("after", cursor.clone()));
        }
        let data = service
            .request("/crm/v3/objects/0-421", &query, None)
            .await?;
        stats.pages += 1;
        for row in data["results"]
            .as_array()
            .ok_or_else(|| fail("apply_reasons_invalid_page"))?
        {
            let Some(app) = parse_app(row, labels) else {
                continue;
            };
            if app.date > today {
                continue;
            }
            dates.push(app.date);
            if app.date >= since && app.has_reason() {
                apps.push(app);
            }
        }
        match data.pointer("/paging/next/after").and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        }) {
            None => return Ok((dates, apps)),
            Some(next) if seen.insert(next.clone()) => after = Some(next),
            _ => return Err(fail("apply_reasons_invalid_paging")),
        }
    }
    Err(fail("apply_reasons_too_many_pages"))
}
/// 存在しない・読めない求人票は飛ばす（1件のために全体を失敗にしない）。
async fn read_listings(
    service: &JobReadService,
    ids: &[String],
    stats: &mut Stats,
) -> Result<BTreeMap<String, Record>, ReadError> {
    let mut found = BTreeMap::new();
    for chunk in ids.chunks(100) {
        let data = service
            .request(
                "/crm/v3/objects/0-420/batch/read",
                &[],
                Some(
                    json!({"properties":["todoufuken","shikuchouson","zhizhong"],
                    "inputs":chunk.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}),
                ),
            )
            .await?;
        stats.listing_calls += 1;
        let rows: Vec<Record> = serde_json::from_value(data["results"].clone())
            .map_err(|_| fail("hubspot_invalid_response"))?;
        found.extend(rows.into_iter().map(|r| (r.id.clone(), r)));
    }
    Ok(found)
}

// ---- 組み立て ----
fn build_row(app: &App, listings: &BTreeMap<String, Record>) -> Row {
    let related: Vec<&Record> = app
        .listing_ids
        .iter()
        .filter_map(|id| listings.get(id))
        .collect();
    let areas: BTreeSet<_> = related
        .iter()
        .map(|r| {
            let a = area(r.value("todoufuken"), r.value("shikuchouson"));
            (a.prefecture, a.municipality)
        })
        .collect();
    let job_area = match areas.len() {
        1 => areas
            .into_iter()
            .next()
            .map(|(prefecture, municipality)| Area {
                prefecture,
                municipality,
            }),
        _ => None,
    };
    // 職種は求人票ごと（求人票に無ければ応募の職種）。食い違えば不明。
    let outcomes: BTreeSet<Option<Option<&'static str>>> = if related.is_empty() {
        BTreeSet::from([app.occupation_text.as_deref().map(classify)])
    } else {
        related
            .iter()
            .map(|r| {
                r.value("zhizhong")
                    .or(app.occupation_text.as_deref())
                    .map(classify)
            })
            .collect()
    };
    let (occupation, status) = if outcomes.len() > 1 {
        (None, "conflict")
    } else {
        match outcomes.into_iter().next().flatten() {
            None => (None, "missing"),
            Some(None) => (None, "unmatched"),
            Some(Some(group)) => (Some(group.to_owned()), "grouped"),
        }
    };
    Row {
        date: app.date.to_string(),
        applicant_area: app.area.clone(),
        job_area,
        occupation,
        occupation_status: status,
        selections: app
            .selections
            .iter()
            .map(|(p, l)| Selection {
                source_property: p.clone(),
                label: l.clone(),
            })
            .collect(),
        texts: app.texts.clone(),
    }
}
fn since_for(today: NaiveDate, months: u32) -> NaiveDate {
    today
        .checked_sub_months(Months::new(months))
        .unwrap_or(NaiveDate::from_ymd_opt(today.year() - 1, 1, 1).unwrap())
}
fn assemble(
    today: NaiveDate,
    dates: &[NaiveDate],
    apps: &[App],
    listings: &BTreeMap<String, Record>,
) -> Report {
    let mut sorted: Vec<&App> = apps.iter().collect();
    sorted.sort_by(|a, b| b.date.cmp(&a.date).then(a.id.cmp(&b.id)));
    let rows: Vec<Row> = sorted.iter().map(|a| build_row(a, listings)).collect();
    let periods = MONTHS
        .iter()
        .map(|&months| {
            let since = since_for(today, months);
            Period {
                months,
                since: since.to_string(),
                total: dates.iter().filter(|d| **d >= since).count(),
                with_reason: apps.iter().filter(|a| a.date >= since).count(),
            }
        })
        .collect();
    Report {
        today: today.to_string(),
        periods,
        occupation_groups: OCCUPATION_GROUPS.to_vec(),
        rows,
    }
}
async fn fetch(service: &JobReadService, today: NaiveDate) -> Result<Report, ReadError> {
    let since = since_for(today, *MONTHS.last().unwrap());
    let mut stats = Stats::default();
    let labels = read_labels(service).await;
    let (dates, apps) = read_apps(service, since, today, labels.as_ref(), &mut stats).await?;
    let ids: Vec<String> = apps
        .iter()
        .flat_map(|a| a.listing_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let listings = read_listings(service, &ids, &mut stats).await?;
    tracing::info!(
        pages = stats.pages,
        listing_calls = stats.listing_calls,
        "job-copy apply reasons reads"
    );
    Ok(assemble(today, &dates, &apps, &listings))
}

// ---- API ----
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = listings::service(&access)?;
    let today = (Utc::now() + chrono::Duration::hours(9)).date_naive();
    let data = answer(service, today).await;
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(data)))
}

async fn answer(service: &JobReadService, today: NaiveDate) -> Value {
    let mut cache = service.apply_reasons.state.lock().await;
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
            let mut cache = service.apply_reasons.state.lock().await;
            match result {
                Ok(report) => {
                    cache.snapshot = Some((today, Instant::now(), Arc::new(report)));
                    cache.built_at = Some(Utc::now());
                    cache.failed_at = None;
                }
                Err(_) => {
                    cache.failed_at = Some(Instant::now());
                    tracing::warn!("job-copy apply reasons refresh failed");
                }
            }
            cache.refreshing = false;
        });
    }
    if let Some((_, _, report)) = &cache.snapshot {
        json!({"status":"ready", "report":report, "refresh_failed":cache.failed_at.is_some(),
            "built_at":cache.built_at, "stale":!fresh, "refreshing":cache.refreshing})
    } else {
        json!({"status":if cache.failed_at.is_some() {"failed"} else {"preparing"}})
    }
}

#[cfg(test)]
#[path = "apply_reasons_tests.rs"]
mod tests;
