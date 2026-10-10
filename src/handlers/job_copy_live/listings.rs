//! Bounded, read-only listing traversal; no Search calls or applicant properties.
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
    "saishuu_csv_kenshutsu_bi",
];
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListingsQuery {
    prefecture: Option<String>,
    title: Option<String>,
    media: Option<String>,
    after: Option<String>,
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
    application_count: Option<usize>,
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
        let occupation = body
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("Indeed表示職種名：")
                    .or_else(|| line.trim().strip_prefix("Indeed表示職種名:"))
            })
            .unwrap_or("")
            .trim()
            .to_owned();
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
        publication_status: (media == "hrh")
            .then(|| {
                record
                    .value("baitai_genjoukyou_hrhakkaa")
                    .map(str::to_owned)
            })
            .flatten(),
        last_csv_detected_at: record.value("saishuu_csv_kenshutsu_bi").map(str::to_owned),
        application_count: None,
    })
}
fn taxonomy(state: &AppState) -> Result<Vec<String>, ReadError> {
    let db = state.indeed_db.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "market_data_unavailable",
    ))?;
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
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<ListingsQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let titles = taxonomy(&state)?;
    if query
        .media
        .as_deref()
        .is_some_and(|v| !["hrh", "airwork"].contains(&v))
        || query
            .title
            .as_ref()
            .is_some_and(|v| v != "unknown" && !titles.contains(v))
        || query
            .prefecture
            .as_ref()
            .is_some_and(|v| !crate::models::job_seeker::PREFECTURE_ORDER.contains(&v.as_str()))
    {
        return Err(ReadError(StatusCode::BAD_REQUEST, "listing_filter_invalid"));
    }
    let service = service(&access)?;
    let mut after = query.after.clone();
    let mut rows = Vec::new();
    let mut scanned = 0;
    let mut seen = BTreeSet::new();
    if let Some(cursor) = &after {
        seen.insert(cursor.clone());
    }
    for _ in 0..5 {
        let mut params = vec![
            ("limit", (50 - rows.len()).to_string()),
            (
                "properties",
                PROPERTIES
                    .iter()
                    .copied()
                    .filter(|p| !["hrh_kyuujinhyou_gazou", "shigotonaiyou"].contains(p))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ];
        if let Some(cursor) = &after {
            valid_id(cursor)?;
            params.push(("after", cursor.clone()));
        }
        let data = service
            .request("/crm/v3/objects/0-420", &params, None)
            .await?;
        let page: Page =
            serde_json::from_value(data).map_err(|_| fail("hubspot_invalid_response"))?;
        if page.results.len() > 50 - rows.len() {
            return Err(fail("hubspot_invalid_response"));
        }
        scanned += page.results.len();
        for record in page.results {
            if let Some(row) = listing(&record, &titles) {
                if query.media.as_ref().is_none_or(|v| v == row.media)
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
                {
                    rows.push(row);
                }
            }
        }
        let next = page.paging.and_then(|p| p.next).map(|n| n.after);
        if next
            .as_ref()
            .is_some_and(|cursor| !seen.insert(cursor.clone()))
        {
            return Err(fail("listing_paging_invalid"));
        }
        after = next;
        if after.is_none() || rows.len() >= 50 {
            break;
        }
    }
    if !rows.is_empty() {
        let counts = service.request("/crm/v4/associations/0-420/0-421/batch/read", &[],
            Some(json!({"inputs":rows.iter().map(|row| json!({"id":row.id})).collect::<Vec<_>>()}))).await;
        if let Ok(data) = counts {
            for row in &mut rows {
                row.application_count = association_count(&data, &row.id);
            }
        }
    }
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"listings":rows,"next_after":after,"scanned":scanned,"titles":titles})),
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
    // Completeness is never asserted: the provider may cap each property's history at 20.
    Ok((result, counts, true))
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
                ("properties", PROPERTIES.join(",")),
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
    let row = listing(&record, &taxonomy(&state)?).ok_or(ReadError(
        StatusCode::UNPROCESSABLE_ENTITY,
        "listing_media_unknown",
    ))?;
    let (versions, counts, may_be_incomplete) = versions(&data, row.media)?;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(
            json!({"listing":row,"versions":versions,"history_counts":counts,"history_may_be_incomplete":may_be_incomplete}),
        ),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(incomplete);
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
        assert_eq!(first.application_count, None);
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
}
