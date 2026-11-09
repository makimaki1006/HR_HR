//! 応募理由 × 地域 × 職種の横断集計。応募者の情報は保存せず、プロセス内に6時間だけ持つ。
//!
//! 理由はヒアリングで選ぶ選択式の項目（応募動機・勤務条件の優先順位①〜③）。文章は詳細として添えるだけで、
//! キーワードで分類しない。選択式の値がある応募だけを Search で読み、全応募の件数は Search の total で数える。
use super::*;
use chrono::{DateTime, Datelike, Months, NaiveDate, Utc};
use std::time::Instant;

const TTL: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY: Duration = Duration::from_secs(60);
const MAX_SEARCH_PAGES: usize = 50;
const MONTHS: [u32; 3] = [1, 3, 6];
/// 応募動機（複数選択）。
const MOTIVE: &str = "oubodouki_1";
/// 勤務条件の優先順位①②③（それぞれ1つ選ぶ）。
const PRIORITIES: [&str; 3] = [
    "kinmujoukennoyuusenjuni_1",
    "kinmujoukennoyuusenjuni_2",
    "kinmujoukennoyuusenjuni_3",
];
pub(super) const SELECT_PROPERTIES: [&str; 4] =
    [MOTIVE, PRIORITIES[0], PRIORITIES[1], PRIORITIES[2]];
const TEXT_PROPERTIES: [&str; 3] = [
    "oubodouki",
    "kinmujoukennoyuusenjuni",
    "honnin_kibou_kinyuuran",
];
const APP_PROPERTIES: [&str; 12] = [
    "yingmuri",
    "hs_createdate",
    "todoufuken",
    "shikuchouson",
    "oubosaki_shokushu",
    MOTIVE,
    PRIORITIES[0],
    PRIORITIES[1],
    PRIORITIES[2],
    "oubodouki",
    "kinmujoukennoyuusenjuni",
    "honnin_kibou_kinyuuran",
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
    /// 選択肢の名前（HubSpot の並び順）。応募動機と優先順位は同じ選択肢を使う。
    options: Vec<String>,
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
        !self.selections.is_empty()
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
    let yingmuri = nonblank(properties, "yingmuri");
    let created = nonblank(properties, super::deal_summary::CREATED);
    let (day, _) =
        super::deal_summary::application_day_of(yingmuri.as_deref(), created.as_deref())?;
    NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok()
}
pub(super) fn is_unset(value: &str, label: Option<&str>) -> bool {
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
    for property in SELECT_PROPERTIES {
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
    Some(App {
        id,
        date,
        area: area(p["todoufuken"].as_str(), p["shikuchouson"].as_str()),
        occupation_text: nonblank(p, "oubosaki_shokushu"),
        selections,
        texts,
        // Search は関連を返さない。read_links で後から入れる。
        listing_ids: BTreeSet::new(),
    })
}

// ---- 読み取り ----
pub(super) struct Options {
    pub(super) labels: applicant_reasons::OptionLabels,
    /// 応募動機の選択肢の名前（displayOrder の順）。
    pub(super) order: Vec<String>,
    pub(super) definitions: BTreeMap<String, Vec<Value>>,
}
pub(super) fn parse_options(data: &Value) -> Option<Options> {
    let mut labels = applicant_reasons::OptionLabels::new();
    let mut order = Vec::new();
    let mut definitions = BTreeMap::new();
    for property in data["results"].as_array()? {
        let Some(name) = property["name"]
            .as_str()
            .filter(|name| SELECT_PROPERTIES.contains(name))
        else {
            continue;
        };
        let mut options: Vec<(i64, String, String)> = property["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| {
                let value = o["value"].as_str()?.trim();
                let label = o["label"].as_str()?.trim();
                (!value.is_empty() && !label.is_empty()).then(|| {
                    (
                        o["displayOrder"].as_i64().unwrap_or(i64::MAX),
                        value.to_owned(),
                        label.to_owned(),
                    )
                })
            })
            .collect();
        options.sort_by_key(|(at, _, _)| *at);
        definitions.insert(
            name.to_owned(),
            options
                .iter()
                .filter(|(_, value, label)| !is_unset(value, Some(label)))
                .map(|(at, value, label)| json!({"value":value,"label":label,"display_order":at}))
                .collect(),
        );
        if name == MOTIVE {
            order = options.iter().map(|(_, _, l)| l.clone()).collect();
        }
        labels.insert(
            name.to_owned(),
            options.into_iter().map(|(_, v, l)| (v, l)).collect(),
        );
    }
    (!labels.is_empty()).then_some(Options {
        labels,
        order,
        definitions,
    })
}
async fn read_options(service: &JobReadService) -> Option<Options> {
    let data = service
        .request(
            "/crm/v3/properties/0-421/batch/read",
            &[],
            Some(json!({"archived":false,"inputs":SELECT_PROPERTIES.iter().map(|name|json!({"name":name})).collect::<Vec<_>>()})),
        )
        .await
        .ok()?;
    parse_options(&data)
}
#[derive(Default)]
struct Stats {
    searches: usize,
    link_calls: usize,
    listing_calls: usize,
}
fn search_total(data: &Value) -> Result<u64, ReadError> {
    data["total"]
        .as_u64()
        .ok_or_else(|| fail("apply_reasons_invalid_search"))
}
/// 選択式の理由が1つでも入っている応募（どれかの項目 HAS_PROPERTY の OR）。
async fn read_apps(
    service: &JobReadService,
    since: NaiveDate,
    today: NaiveDate,
    labels: Option<&applicant_reasons::OptionLabels>,
    stats: &mut Stats,
) -> Result<Vec<App>, ReadError> {
    // 作成日は応募日より前にならないので、作成日が起点以降なら期間の応募を取りこぼさない
    // （古い応募が後から作られた分は、下で応募日を見て外す）。
    let created_since = jst_ms(since).to_string();
    let groups: Vec<Value> = SELECT_PROPERTIES
        .iter()
        .map(|p| {
            json!({"filters":[{"propertyName":p,"operator":"HAS_PROPERTY"},
                {"propertyName":"hs_createdate","operator":"GTE","value":created_since}]})
        })
        .collect();
    let mut apps = Vec::new();
    let mut ids = BTreeSet::new();
    let mut after: Option<Value> = None;
    for _ in 0..MAX_SEARCH_PAGES {
        let mut body = json!({"filterGroups":groups,"properties":APP_PROPERTIES,"limit":200,
            "sorts":[{"propertyName":"hs_object_id","direction":"ASCENDING"}]});
        if let Some(cursor) = &after {
            body["after"] = cursor.clone();
        }
        let data = service
            .request("/crm/v3/objects/0-421/search", &[], Some(body))
            .await?;
        stats.searches += 1;
        if search_total(&data)? > 10_000 {
            return Err(fail("apply_reasons_search_limit"));
        }
        for row in data["results"]
            .as_array()
            .ok_or_else(|| fail("apply_reasons_invalid_search"))?
        {
            let Some(app) = parse_app(row, labels) else {
                continue;
            };
            if app.date > today || app.date < since || !app.has_reason() {
                continue;
            }
            if ids.insert(app.id.clone()) {
                apps.push(app);
            }
        }
        match data.pointer("/paging/next/after").cloned() {
            None => return Ok(apps),
            Some(next) if after.as_ref() != Some(&next) => after = Some(next),
            _ => return Err(fail("apply_reasons_invalid_paging")),
        }
    }
    Err(fail("apply_reasons_too_many_pages"))
}
fn day_ms(d: NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp_millis()
}
/// JST のその日の 0 時 = 前日 15 時 (UTC)。
fn jst_ms(d: NaiveDate) -> i64 {
    day_ms(d) - 9 * 60 * 60 * 1000
}
/// 応募日 (yingmuri、無ければ作成日の JST の日付) が since 以降 today 以前の応募を数える Search。
fn count_body(since: NaiveDate, today: NaiveDate) -> Value {
    let end = today + chrono::Duration::days(1);
    json!({"filterGroups":[
        {"filters":[
            {"propertyName":"yingmuri","operator":"GTE","value":day_ms(since).to_string()},
            {"propertyName":"yingmuri","operator":"LT","value":day_ms(end).to_string()}]},
        {"filters":[
            {"propertyName":"yingmuri","operator":"NOT_HAS_PROPERTY"},
            {"propertyName":"hs_createdate","operator":"GTE","value":jst_ms(since).to_string()},
            {"propertyName":"hs_createdate","operator":"LT","value":jst_ms(end).to_string()}]}
    ],"properties":["hs_object_id"],"limit":1})
}
async fn count_apps(
    service: &JobReadService,
    since: NaiveDate,
    today: NaiveDate,
    stats: &mut Stats,
) -> Result<usize, ReadError> {
    let data = service
        .request(
            "/crm/v3/objects/0-421/search",
            &[],
            Some(count_body(since, today)),
        )
        .await?;
    stats.searches += 1;
    Ok(search_total(&data)? as usize)
}
fn no_association_ids(error: &Value) -> Option<Vec<String>> {
    if error["subCategory"].as_str() != Some("crm.associations.NO_ASSOCIATIONS_FOUND") {
        return None;
    }
    error
        .pointer("/context/fromObjectId")?
        .as_array()?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        })
        .collect()
}
/// 応募 → 求人票。関連の無い応募は空。欠けた応答は失敗にする（関連なしと見なさない）。
async fn read_links(
    service: &JobReadService,
    ids: &[String],
    stats: &mut Stats,
) -> Result<BTreeMap<String, BTreeSet<String>>, ReadError> {
    let mut links = BTreeMap::<String, BTreeSet<String>>::new();
    for chunk in ids.chunks(100) {
        let data = service
            .request(
                "/crm/v4/associations/0-421/0-420/batch/read",
                &[],
                Some(json!({"inputs":chunk.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()})),
            )
            .await?;
        stats.link_calls += 1;
        let expected: BTreeSet<&str> = chunk.iter().map(String::as_str).collect();
        let mut seen = BTreeSet::new();
        for error in data["errors"].as_array().into_iter().flatten() {
            for id in no_association_ids(error).ok_or_else(|| fail("hubspot_partial_read"))? {
                if !expected.contains(id.as_str()) || !seen.insert(id.clone()) {
                    return Err(fail("hubspot_invalid_response"));
                }
                links.entry(id).or_default();
            }
        }
        for row in data["results"]
            .as_array()
            .ok_or_else(|| fail("hubspot_invalid_response"))?
        {
            let id = row
                .pointer("/from/id")
                .and_then(Value::as_str)
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            if !expected.contains(id) || !seen.insert(id.to_owned()) || row.get("paging").is_some()
            {
                return Err(fail("hubspot_partial_read"));
            }
            let entry = links.entry(id.to_owned()).or_default();
            for target in row["to"]
                .as_array()
                .ok_or_else(|| fail("hubspot_invalid_response"))?
            {
                let target = target["toObjectId"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| target["toObjectId"].as_u64().map(|n| n.to_string()))
                    .ok_or_else(|| fail("hubspot_invalid_response"))?;
                entry.insert(target);
            }
        }
        if seen.len() != expected.len() {
            return Err(fail("hubspot_partial_read"));
        }
    }
    Ok(links)
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
    options: Vec<String>,
    totals: &[usize],
    apps: &[App],
    listings: &BTreeMap<String, Record>,
) -> Report {
    let mut sorted: Vec<&App> = apps.iter().collect();
    sorted.sort_by(|a, b| b.date.cmp(&a.date).then(a.id.cmp(&b.id)));
    let rows: Vec<Row> = sorted.iter().map(|a| build_row(a, listings)).collect();
    let periods = MONTHS
        .iter()
        .zip(totals)
        .map(|(&months, &total)| {
            let since = since_for(today, months);
            Period {
                months,
                since: since.to_string(),
                total,
                with_reason: apps.iter().filter(|a| a.date >= since).count(),
            }
        })
        .collect();
    Report {
        today: today.to_string(),
        options,
        periods,
        occupation_groups: OCCUPATION_GROUPS.to_vec(),
        rows,
    }
}
async fn fetch(service: &JobReadService, today: NaiveDate) -> Result<Report, ReadError> {
    // メモリ計測: 応募理由の集計の作り直し 1 回
    let _task = crate::mem_watch::task("job_copy.apply_reasons");
    let since = since_for(today, *MONTHS.last().unwrap());
    let mut stats = Stats::default();
    let options = read_options(service).await;
    let mut apps = read_apps(
        service,
        since,
        today,
        options.as_ref().map(|o| &o.labels),
        &mut stats,
    )
    .await?;
    let mut totals = Vec::new();
    for months in MONTHS {
        totals.push(count_apps(service, since_for(today, months), today, &mut stats).await?);
    }
    let app_ids: Vec<String> = apps.iter().map(|a| a.id.clone()).collect();
    let mut links = read_links(service, &app_ids, &mut stats).await?;
    for app in &mut apps {
        app.listing_ids = links.remove(&app.id).unwrap_or_default();
    }
    let ids: Vec<String> = apps
        .iter()
        .flat_map(|a| a.listing_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let listings = read_listings(service, &ids, &mut stats).await?;
    tracing::info!(
        searches = stats.searches,
        link_calls = stats.link_calls,
        listing_calls = stats.listing_calls,
        apps = apps.len(),
        "job-copy apply reasons reads"
    );
    let order = options.map(|o| o.order).unwrap_or_default();
    Ok(assemble(today, order, &totals, &apps, &listings))
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
