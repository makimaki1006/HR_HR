//! 応募の既存 batch read をカードへ変換。定義は6時間、担当者は既存一覧で共用する。
use super::*;
use crate::hubspot::HubSpotRecord;

pub(super) const LISTING_PROPERTIES: &[&str] = &[
    "hs_name",
    "customer_sheet_url",
    "url_hrhakkaa",
    "url_airwork",
    "jobcopy_app_url",
    "kyuujin_status",
    "baitai_genjoukyou_hrhakkaa",
    "baitai_genjoukyou_airwork",
    "koukai_kaishi_nichiji",
    "koukai_shuuryou_nichiji",
    "keisaishuuryounichi",
    "qinwude",
    "kyuuyokeitai",
    "zhizhong",
    "kinmujikan",
    "guyongxingtai",
];
pub(super) async fn definitions(
    ctx: &CrmCtx,
    client: &HubSpotClient,
) -> Result<Value, HubSpotError> {
    let state = &ctx.applicants;
    if let Some(v) = state.related_definitions.get(&()) {
        return Ok(v);
    }
    let _lock = state.related_definition_lock.lock().await;
    if let Some(v) = state.related_definitions.get(&()) {
        return Ok(v);
    }
    let (listing, mut deal, pipelines) = tokio::try_join!(
        raw(client, "/crm/v3/properties/0-420"),
        raw(client, "/crm/v3/properties/deals"),
        raw(client, "/crm/v3/pipelines/deals"),
    )?;
    if [&listing, &deal, &pipelines]
        .iter()
        .any(|v| !v["results"].is_array())
    {
        return Err(HubSpotError::Decode("related definitions".into()));
    }
    for def in deal["results"].as_array_mut().unwrap() {
        if let Some(label) = crate::crm::label_ja::property_label("deals", &text(def, "name"), def)
        {
            def["label"] = json!(label);
        }
    }
    let defs = json!({"listing":{"properties":listing["results"]},"deal":{"properties":deal["results"],"pipelines":pipelines["results"]}});
    state.related_definitions.insert((), defs.clone());
    Ok(defs)
}
fn fields(
    record: &HubSpotRecord,
    defs: &Value,
    names: &[&str],
    owners: Option<&crate::crm::owners::CrmOwnersResponse>,
    applicant: &Value,
) -> Vec<ApplicantField> {
    names
        .iter()
        .filter_map(|name| {
            let raw = record
                .properties
                .get(*name)
                .and_then(|v| v.as_deref())?
                .trim();
            if raw.is_empty()
                || matches!(
                    *name,
                    "hs_name" | "dealname" | "customer_sheet_url" | "createdate"
                )
            {
                return None;
            }
            // 中央の対応ルールにある同じ意味の項目は重ねない。
            let rule = match *name {
                "oubohoukokusaki" => "anmokuchi_oubohoukokusaki",
                "ashigirijouken" => "anmokuchi_ashigirijouken",
                _ => "",
            };
            if !rule.is_empty() && !text(applicant, rule).is_empty() {
                return None;
            }
            let def = values(&defs["properties"]).find(|d| d["name"] == *name)?;
            if def["hidden"] == true
                || def["archived"] == true
                || def["dataSensitivity"]
                    .as_str()
                    .is_some_and(|s| s != "non_sensitive")
            {
                return None;
            }
            let label = text(def, "label");
            if label.is_empty() {
                return None;
            }
            let value = if *name == "hubspot_owner_id" {
                owners
                    .and_then(|o| o.owners.iter().find(|o| o.id == raw))
                    .map(|o| o.name.clone())
                    .unwrap_or_else(|| "担当者を確認できません".into())
            } else if *name == "dealstage" || *name == "pipeline" {
                values(&defs["pipelines"])
                    .filter(|p| {
                        record
                            .properties
                            .get("pipeline")
                            .and_then(|v| v.as_deref())
                            .is_none_or(|id| p["id"] == id)
                    })
                    .flat_map(|p| {
                        if *name == "pipeline" {
                            vec![p]
                        } else {
                            values(&p["stages"]).collect()
                        }
                    })
                    .find(|s| s["id"] == raw)
                    .map(|s| text(s, "label"))
                    .unwrap_or_else(|| "ステージを確認できません".into())
            } else if matches!(text(def, "type").as_str(), "date" | "datetime") {
                let ms = date_ms(raw);
                chrono::DateTime::from_timestamp_millis(ms)
                    .filter(|_| ms != 0)
                    .map(|d| {
                        d.with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap())
                            .format("%Y/%m/%d")
                            .to_string()
                    })
                    .unwrap_or_else(|| raw.into())
            } else {
                display(defs, name, raw)
            };
            Some(ApplicantField { label, value })
        })
        .collect()
}
pub(super) async fn cards(
    ctx: &CrmCtx,
    client: &HubSpotClient,
    jobs: &[HubSpotRecord],
    related: &report_sheet::RelatedDeals,
    applicant: &Value,
    definitions: Result<Value, HubSpotError>,
) -> (
    Vec<ApplicantRelatedCard>,
    Vec<ApplicantRelatedCard>,
    Option<String>,
) {
    let mut notice = related
        .unavailable
        .then(|| "関連する取引を読み込めませんでした。最新にして再試行してください。".into());
    if jobs.is_empty() && related.deals.is_empty() {
        return (vec![], vec![], notice);
    }
    // 求人票本体が欠け、取引だけ取得できた場合も従来どおり定義を読む。
    let definitions = match definitions {
        Ok(Value::Null) => self::definitions(ctx, client).await,
        other => other,
    };
    let defs = match definitions {
        Ok(v) => v,
        Err(_) => {
            notice = Some(
                "関連する項目の一覧を読み込めませんでした。最新にして再試行してください。".into(),
            );
            Value::Null
        }
    };
    let owners = if related.deals.iter().any(|d| {
        d.properties
            .get("hubspot_owner_id")
            .and_then(|v| v.as_deref())
            .is_some_and(|s| !s.is_empty())
    }) {
        match ctx.queue.owner_list.get(client).await {
            Ok(v) => Some(v),
            Err(_) => {
                notice = Some(
                    "取引の担当者名を読み込めませんでした。最新にして再試行してください。".into(),
                );
                None
            }
        }
    } else {
        None
    };
    let make = |r: &HubSpotRecord, listing: bool| {
        let p = json!(r.properties);
        ApplicantRelatedCard {
            id: r.id.clone(),
            title: text(&p, if listing { "hs_name" } else { "dealname" }),
            deep_link: link(if listing { "0-420" } else { "0-3" }, &r.id),
            jobcopy_url: listing
                .then(|| http_url(&text(&p, "jobcopy_app_url")))
                .flatten(),
            current: !listing && related.current.as_ref() == Some(&r.id),
            fields: fields(
                r,
                &defs[if listing { "listing" } else { "deal" }],
                if listing {
                    LISTING_PROPERTIES
                } else {
                    report_sheet::PROPERTIES
                },
                owners.as_deref(),
                applicant,
            ),
        }
    };
    (
        jobs.iter().map(|r| make(r, true)).collect(),
        related.deals.iter().map(|r| make(r, false)).collect(),
        notice,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fields_omit_empty_duplicate_rules_and_unknown_owner_ids() {
        let record = HubSpotRecord {
            id: "700".into(),
            created_at: None,
            updated_at: None,
            archived: false,
            properties: [
                ("oubohoukokusaki", "架空窓口"),
                ("ashigirijouken", "資格を確認"),
                ("hubspot_owner_id", "600"),
                ("contract_start_date", " "),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), Some(v.into())))
            .collect(),
        };
        let defs = json!({"properties":[
            {"name":"oubohoukokusaki","label":"応募報告先","type":"string"},
            {"name":"ashigirijouken","label":"足切り条件","type":"string"},
            {"name":"hubspot_owner_id","label":"取引担当者","type":"enumeration"},
            {"name":"contract_start_date","label":"契約開始日","type":"date"}
        ]});
        let result = fields(
            &record,
            &defs,
            report_sheet::PROPERTIES,
            None,
            &json!({"anmokuchi_oubohoukokusaki":"応募側の窓口", "anmokuchi_ashigirijouken":"応募側の条件"}),
        );
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!([{"label":"取引担当者","value":"担当者を確認できません"}])
        );
    }
}
