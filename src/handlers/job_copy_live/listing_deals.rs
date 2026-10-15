//! Deal batch index reads and column-only comparison summaries.
use super::*;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Deal {
    pub(super) id: String,
    pub(super) name: Option<String>,
    /// 取引のステージ ID と作成日（YYYY-MM-DD）。ホームの顧客表だけが使う。画面には返さない。
    #[serde(skip)]
    pub(super) pipeline: Option<String>,
    #[serde(skip)]
    pub(super) stage: Option<String>,
    #[serde(skip)]
    pub(super) created_at: Option<String>,
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
    let holidays = get("休日休暇").or_else(|| {
        ["仕事情報補足", "自由項目"].into_iter().find_map(|prefix| {
            (1..=4).find_map(|n| {
                let title = fields.get(format!("{prefix}{n}のタイトル").as_str())?;
                ["休日", "休日・休暇", "休日休暇", "休暇"]
                    .contains(&title.trim())
                    .then(|| get(&format!("{prefix}{n}の内容")))
                    .flatten()
            })
        })
    });
    let air = |key| record.value(key).map(str::to_owned);
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
        salary_kind: if media == "hrh" {
            get("給与形態").map(|v| crate::job_gen::copy_display::code("給与形態", &v))
        } else {
            air("kyuuyokeitai").filter(|v| v != "unset")
        },
        salary_min: get("基本給与 最小"),
        salary_max: get("基本給与 最大"),
        hours: if media == "hrh" {
            get("勤務時間")
        } else {
            air("kinmujikan")
        },
        holidays: if media == "hrh" {
            holidays
        } else {
            air("kyuujitsu")
        },
        body_changed_at: None,
    }
}
pub(super) fn publication(record: &Record, media: &str) -> Option<String> {
    Some(
        crate::job_gen::publication::label(
            record.value("kyuujin_status"),
            record.value(if media == "hrh" {
                "baitai_genjoukyou_hrhakkaa"
            } else {
                "baitai_genjoukyou_airwork"
            }),
            media,
        )
        .to_owned(),
    )
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
fn no_association_ids(error: &Value) -> Option<Vec<String>> {
    if error["subCategory"].as_str() != Some("crm.associations.NO_ASSOCIATIONS_FOUND") {
        return None;
    }
    let ids: Vec<String> = error
        .pointer("/context/fromObjectId")?
        .as_array()?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        })
        .collect::<Option<_>>()?;
    (!ids.is_empty()).then_some(ids)
}
// Strict batch paging: missing/error rows never turn into a false "no deal" or zero count.
/// Four batch reads at most; continuations stay within their original chunk.
pub(super) async fn index_associations(
    reader: &IndexReader,
    ids: &[String],
    to: &str,
) -> Result<BTreeMap<String, Vec<String>>, ReadError> {
    let mut result = BTreeMap::new();
    for group in ids.chunks(4_000) {
        let mut tasks = tokio::task::JoinSet::new();
        for chunk in group.chunks(1_000) {
            let (reader, ids, to) = (reader.clone(), chunk.to_vec(), to.to_owned());
            tasks.spawn(async move { index_associations_chunk(&reader, &ids, &to).await });
        }
        while let Some(result_chunk) = tasks.join_next().await {
            result.extend(result_chunk.map_err(|_| fail("hubspot_read_failed"))??);
        }
    }
    Ok(result)
}

async fn index_associations_chunk(
    reader: &IndexReader,
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
            let data = reader
                .read(
                    &format!("/crm/v4/associations/0-420/{to}/batch/read"),
                    &[],
                    Some(&json!({"inputs":pending})),
                    &super::super::listing_retry::DELAYS,
                )
                .await?;
            let mut actual = BTreeSet::new();
            // HubSpot reports "no associations" as an error row per object, not as `to: []`.
            // Only that sub-category means "none"; any other error is still a failed read.
            for error in data
                .get("errors")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let ids = no_association_ids(error).ok_or_else(|| fail("hubspot_partial_read"))?;
                for id in ids {
                    if !expected.contains(&id) || !actual.insert(id.clone()) {
                        return Err(fail("hubspot_invalid_response"));
                    }
                    targets.entry(id).or_default();
                }
            }
            let rows = data["results"]
                .as_array()
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
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
/// Deal names and home columns: batches of 100, at most four in flight.
pub(super) async fn index_deal_records(
    reader: &IndexReader,
    ids: &[String],
) -> Result<Vec<Record>, ReadError> {
    let mut records = Vec::new();
    for group in ids.chunks(400) {
        let mut tasks = tokio::task::JoinSet::new();
        for chunk in group.chunks(100) {
            let (reader, ids) = (reader.clone(), chunk.to_vec());
            tasks.spawn(async move {
                let body = json!({
                    "properties": ["dealname", "pipeline", "dealstage", "createdate"],
                    "inputs": ids.iter().map(|id| json!({"id": id})).collect::<Vec<_>>()
                });
                let data = reader
                    .read(
                        "/crm/v3/objects/deals/batch/read",
                        &[],
                        Some(&body),
                        &super::super::listing_retry::DELAYS,
                    )
                    .await?;
                let page: Page = serde_json::from_value(data.clone())
                    .map_err(|_| fail("hubspot_invalid_response"))?;
                if data
                    .get("errors")
                    .and_then(Value::as_array)
                    .is_some_and(|v| !v.is_empty())
                    || page.results.len() != ids.len()
                    || page.results.iter().map(|r| &r.id).collect::<BTreeSet<_>>()
                        != ids.iter().collect::<BTreeSet<_>>()
                {
                    return Err(fail("hubspot_partial_read"));
                }
                Ok::<_, ReadError>(page.results)
            });
        }
        while let Some(result) = tasks.join_next().await {
            records.extend(result.map_err(|_| fail("hubspot_read_failed"))??);
        }
    }
    Ok(records)
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
        let listing = listing(record).ok_or_else(|| fail("listing_media_invalid"))?;
        let body = record.value(if listing.media == "hrh" {
            "hrh_kyuujinhyou_honbun"
        } else {
            "shigotonaiyou"
        });
        jobs.push(json!({"fields":comparison_fields(record, listing.media),"listing":listing,"body":body}));
    }
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"jobs":jobs})),
    ))
}
/// Media-independent display keys. This does not identify the same job across media.
pub(super) fn comparison_fields(record: &Record, media: &str) -> BTreeMap<String, String> {
    if media == "hrh" {
        let mut fields: BTreeMap<String, String> =
            super::hrh_copy::fields84(record.value("hrh_kyuujinhyou_honbun").unwrap_or(""))
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect();
        let conditions = summary(record, media);
        for (key, value) in [
            ("勤務地", conditions.location),
            ("休日・休暇", conditions.holidays),
        ] {
            if let Some(value) = value {
                fields.insert(key.into(), value);
            }
        }
        return fields;
    }
    [
        ("案件名", "hs_name"),
        ("仕事内容", "shigotonaiyou"),
        ("Indeed表示職種名", "zhizhong"),
        ("勤務地", "qinwude"),
        ("給与形態", "kyuuyokeitai"),
        ("勤務時間", "kinmujikan"),
        ("休日・休暇", "kyuujitsu"),
        ("雇用形態", "guyongxingtai"),
        ("福利厚生", "fukurikousei"),
    ]
    .into_iter()
    .filter_map(|(key, prop)| {
        record.value(prop).filter(|v| *v != "unset").map(|v| {
            (
                key.into(),
                if prop == "guyongxingtai" {
                    match v {
                        "full_time" => "正社員",
                        "contract" => "契約社員",
                        "outsourced" => "業務委託",
                        "part_time" => "パート・アルバイト",
                        "other" => "その他",
                        _ => "確認が必要",
                    }
                    .into()
                } else {
                    v.into()
                },
            )
        })
    })
    .collect()
}
/// History is intentionally fetched only for the requested visible rows, never the index.
pub(super) async fn body_dates(
    service: &JobReadService,
    ids: &[String],
) -> Result<Value, ReadError> {
    let mut dates = BTreeMap::new();
    for chunk in ids.chunks(50) {
        let data = service.request("/crm/v3/objects/0-420/batch/read", &[], Some(json!({"inputs":chunk.iter().map(|id| json!({"id":id})).collect::<Vec<_>>(), "properties":["id_hrhakkaa","id_airwork","hrh_kyuujinhyou_honbun","shigotonaiyou"], "propertiesWithHistory":["hrh_kyuujinhyou_honbun","shigotonaiyou"]}))).await?;
        let page: Page =
            serde_json::from_value(data.clone()).map_err(|_| fail("hubspot_invalid_response"))?;
        if data
            .get("errors")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
            || page.results.iter().map(|r| &r.id).collect::<BTreeSet<_>>()
                != chunk.iter().collect::<BTreeSet<_>>()
            || page.results.len() != chunk.len()
        {
            return Err(fail("hubspot_partial_read"));
        }
        for record in &page.results {
            let media = match (record.value("id_hrhakkaa"), record.value("id_airwork")) {
                (Some(_), None) => "hrh",
                (None, Some(_)) => "airwork",
                _ => return Err(fail("listing_media_invalid")),
            };
            let raw = data["results"]
                .as_array()
                .and_then(|r| r.iter().find(|v| v["id"].as_str() == Some(&record.id)))
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            dates.insert(record.id.clone(), body_changed_at(raw, media));
        }
    }
    Ok(json!({"dates":dates}))
}
pub(in crate::handlers::job_copy_live) async fn read_body_dates(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<ComparisonQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let ids: Vec<String> = query.ids.split(',').map(str::to_owned).collect();
    if ids.is_empty() || ids.len() > 50 || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(ReadError(StatusCode::BAD_REQUEST, "listing_filter_invalid"));
    }
    for id in &ids {
        valid_id(id)?;
    }
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(body_dates(service(&access)?, &ids).await?),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn job_copy_only_no_association_errors_count_as_none() {
        let none = json!({"status":"error","category":"OBJECT_NOT_FOUND","subCategory":"crm.associations.NO_ASSOCIATIONS_FOUND","context":{"fromObjectId":["548568732444", 7]}});
        assert_eq!(
            no_association_ids(&none),
            Some(vec!["548568732444".into(), "7".into()])
        );
        let other = json!({"status":"error","category":"RATE_LIMITS","subCategory":"crm.associations.SOMETHING","context":{"fromObjectId":["1"]}});
        assert_eq!(no_association_ids(&other), None);
        let no_ids = json!({"subCategory":"crm.associations.NO_ASSOCIATIONS_FOUND","context":{}});
        assert_eq!(no_association_ids(&no_ids), None);
    }
}
