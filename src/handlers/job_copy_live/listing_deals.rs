//! Deal batch index reads and column-only comparison summaries.
use super::*;
#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Summary {
    pub(super) occupation: Option<String>,
    pub(super) location: Option<String>,
    pub(super) salary_kind: Option<String>,
    pub(super) salary_min: Option<String>,
    pub(super) salary_max: Option<String>,
    pub(super) hours: Option<String>,
    pub(super) holidays: Option<String>,
    pub(super) body_changed_at: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub(super) struct Deal {
    pub(super) id: String,
    pub(super) name: Option<String>,
    pub(super) count: usize,
}
pub(super) fn summary(record: &Record, media: &str) -> Summary {
    let body = record
        .value(if media == "hrh" {
            "hrh_kyuujinhyou_honbun"
        } else {
            "shigotonaiyou"
        })
        .unwrap_or("");
    let fields = super::hrh_copy::fields(body);
    let get = |key: &str| fields.get(key).filter(|v| !v.is_empty()).cloned();
    // Free-text salary/hours are deliberately not interpreted.
    let holidays = (1..=4).find_map(|n| {
        let title = fields.get(format!("自由項目{n}のタイトル").as_str())?;
        ["休日", "休日・休暇", "休日休暇", "休暇"]
            .contains(&title.trim())
            .then(|| get(&format!("自由項目{n}の内容")))
            .flatten()
    });
    Summary {
        occupation: if media == "hrh" {
            get("Indeed表示職種名")
        } else {
            record.value("zhizhong").map(str::to_owned)
        },
        location: if media == "hrh" {
            let parts: Vec<_> = [record.value("todoufuken"), record.value("shikuchouson")]
                .into_iter()
                .flatten()
                .collect();
            (!parts.is_empty()).then(|| parts.join(""))
        } else {
            record.value("qinwude").map(str::to_owned)
        },
        salary_kind: get("給与形態"),
        salary_min: get("基本給与 最小"),
        salary_max: get("基本給与 最大"),
        hours: get("勤務時間"),
        holidays,
        body_changed_at: None,
    }
}
pub(super) fn publication(raw: Option<&str>) -> Option<String> {
    raw.map(|v| match v.trim() {
        "公開" | "公開中" | "掲載中" | "非公開" | "公開終了" | "公開開始前" | "掲載終了"
        | "募集中" | "募集終了" => v.trim().to_owned(),
        _ => "不明".to_owned(),
    })
}
pub(super) fn body_changed_at(data: &Value, media: &str) -> Option<String> {
    let key = if media == "hrh" {
        "hrh_kyuujinhyou_honbun"
    } else {
        "shigotonaiyou"
    };
    let mut history: Vec<_> = data["propertiesWithHistory"][key]
        .as_array()?
        .iter()
        .map(|v| {
            Some((
                chrono::DateTime::parse_from_rfc3339(v["timestamp"].as_str()?).ok()?,
                v["value"].as_str()?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    history.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    if history
        .windows(2)
        .any(|w| w[0].0 == w[1].0 && w[0].1 != w[1].1)
    {
        return None;
    }
    let current = data["properties"][key].as_str()?;
    if history.first()?.1 != current {
        return None;
    }
    let end = history
        .iter()
        .take_while(|(_, value)| *value == current)
        .last()?;
    // A truncated all-equal history cannot establish when the value changed.
    if history.len() >= 20 && history.iter().all(|(_, value)| *value == current) {
        return None;
    }
    Some(end.0.to_rfc3339())
}
// Strict batch paging: missing/error rows never turn into a false "no deal" or zero count.
pub(super) async fn index_associations(
    service: &JobReadService,
    client: &crate::hubspot::HubSpotClient,
    ids: &[String],
    to: &str,
) -> Result<BTreeMap<String, Vec<String>>, ReadError> {
    let mut result = BTreeMap::new();
    for chunk in ids.chunks(1000) {
        let mut pending: Vec<_> = chunk.iter().map(|id| json!({"id":id})).collect();
        let mut targets = BTreeMap::<String, BTreeSet<String>>::new();
        let mut seen = BTreeSet::new();
        while !pending.is_empty() {
            let expected: BTreeSet<_> = pending
                .iter()
                .filter_map(|v| v["id"].as_str().map(str::to_owned))
                .collect();
            let data = service
                .request_with_client(
                    client,
                    &format!("/crm/v4/associations/0-420/{to}/batch/read"),
                    &[],
                    Some(json!({"inputs":pending})),
                )
                .await?;
            if data
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
            {
                return Err(fail("hubspot_partial_read"));
            }
            let rows = data["results"]
                .as_array()
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            let mut actual = BTreeSet::new();
            pending = vec![];
            for row in rows {
                let id = row
                    .pointer("/from/id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| fail("hubspot_invalid_response"))?;
                if !expected.contains(id) || !actual.insert(id.to_owned()) {
                    return Err(fail("hubspot_invalid_response"));
                }
                let entry = targets.entry(id.into()).or_default();
                for target in row["to"]
                    .as_array()
                    .ok_or_else(|| fail("hubspot_invalid_response"))?
                {
                    let target = target["toObjectId"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| target["toObjectId"].as_u64().map(|v| v.to_string()))
                        .ok_or_else(|| fail("hubspot_invalid_response"))?;
                    valid_id(&target).map_err(|_| fail("hubspot_invalid_response"))?;
                    entry.insert(target);
                }
                if let Some(next) = row.pointer("/paging/next") {
                    let after = next["after"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| next["after"].as_u64().map(|v| v.to_string()))
                        .ok_or_else(|| fail("association_paging_invalid"))?;
                    if !seen.insert((id.to_owned(), after.clone())) {
                        return Err(fail("association_paging_invalid"));
                    }
                    pending.push(json!({"id":id,"after":after}));
                } else if row.get("paging").is_some() {
                    return Err(fail("association_paging_invalid"));
                }
            }
            if actual != expected {
                return Err(fail("hubspot_partial_read"));
            }
        }
        result.extend(
            targets
                .into_iter()
                .map(|(id, ids)| (id, ids.into_iter().collect())),
        );
    }
    Ok(result)
}
pub(super) fn deal_candidates(index: &Index, search: &str) -> Vec<Deal> {
    let search = normalize(search);
    let mut deals: Vec<_> = index
        .deals
        .iter()
        .filter(|d| {
            search.is_empty()
                || normalize(d.name.as_deref().unwrap_or("取引名未取得")).contains(&search)
        })
        .cloned()
        .collect();
    deals.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    deals.truncate(30);
    deals
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::handlers::job_copy_live) struct ComparisonQuery {
    ids: String,
}
pub(in crate::handlers::job_copy_live) async fn read_comparison(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<ComparisonQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let ids: Vec<String> = query.ids.split(',').map(str::to_owned).collect();
    if !(2..=5).contains(&ids.len()) || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(ReadError(StatusCode::BAD_REQUEST, "listing_filter_invalid"));
    }
    for id in &ids {
        valid_id(id)?;
    }
    let service = service(&access)?;
    let records = service.batch("0-420", &ids, PROPERTIES).await?;
    let mut jobs = Vec::new();
    for id in ids {
        let record = records
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| fail("hubspot_partial_read"))?;
        let listing = listing(record, &[]).ok_or_else(|| fail("listing_media_invalid"))?;
        let body = record.value(if listing.media == "hrh" {
            "hrh_kyuujinhyou_honbun"
        } else {
            "shigotonaiyou"
        });
        jobs.push(json!({"listing":listing,"body":body}));
    }
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"jobs":jobs})),
    ))
}
