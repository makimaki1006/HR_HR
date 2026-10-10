//! 関連を最後までたどり、読み取った原本から案を作る。HubSpotへの書き込みはしない。
use super::*;
use crate::job_gen::rikurozi::{self as draft, Comparison, Source};
use axum::extract::Path as AxumPath;
const PROPERTIES: &[&str] = &[
    "hs_name",
    "id_hrhakkaa",
    "id_shop_hrhakkaa",
    "hrh_kyuujinhyou_honbun",
    "hrh_kyuujinhyou_gazou",
    "id_airwork",
    "shigotonaiyou",
    "baitai_genjoukyou_airwork",
    "todoufuken",
    "shikuchouson",
];
fn publication(value: Option<&str>) -> Option<&'static str> {
    match value.map(str::trim) {
        Some("公開" | "掲載中") => Some("掲載中"),
        Some("非公開" | "公開終了" | "掲載終了" | "公開開始前") => None,
        _ => Some("掲載中か不明"),
    }
}
impl JobReadService {
    async fn rikurozi_source(&self, id: &str) -> Result<Source, ReadError> {
        valid_id(id)?;
        let record = self
            .batch("0-420", &[id.to_owned()], PROPERTIES)
            .await?
            .remove(0);
        let job_id = record.value("id_hrhakkaa").ok_or(ReadError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "hrh_source_required",
        ))?;
        let shop = record.value("id_shop_hrhakkaa").ok_or(ReadError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "hrh_shop_missing",
        ))?;
        let body = record.value("hrh_kyuujinhyou_honbun").ok_or(ReadError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "hrh_body_missing",
        ))?;
        let parsed = super::hrh_copy::fields(body);
        if parsed.is_empty() {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "hrh_fields_missing",
            ));
        }
        let mut row: draft::Row = crate::job_gen::hrhacker::HRHACKER_COLUMNS
            .iter()
            .map(|c| (c.to_string(), parsed.get(*c).cloned().unwrap_or_default()))
            .collect();
        row.insert("求人id".into(), job_id.into());
        row.insert("店舗id".into(), shop.into());
        // The image property retains slots; never shift image3 into an empty image2.
        let images = super::hrh_copy::fields(record.value("hrh_kyuujinhyou_gazou").unwrap_or(""));
        for column in ["画像1", "画像2", "画像3"] {
            row.insert(
                column.into(),
                images
                    .get(column)
                    .filter(|url| url.starts_with("https://"))
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        let deals = self.all_associations("0-420", id, "deals").await?;
        let mut companies = BTreeSet::new();
        for targets in self
            .association_map("deals", &deals, "companies")
            .await?
            .into_values()
        {
            companies.extend(targets);
        }
        if companies.len() != 1 {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "customer_relation_unknown",
            ));
        }
        let company = companies.first().expect("one company");
        let deals = self.all_associations("companies", company, "deals").await?;
        let mut ids = BTreeSet::new();
        for targets in self
            .association_map("deals", &deals, "0-420")
            .await?
            .into_values()
        {
            ids.extend(targets);
        }
        if !ids.contains(id) {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "customer_relation_unknown",
            ));
        }
        // Bound the complete scope; never silently use a partial comparison set.
        if ids.len() > 2000 {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "comparison_scope_too_large",
            ));
        }
        let rows = self
            .batch("0-420", &ids.into_iter().collect::<Vec<_>>(), PROPERTIES)
            .await?;
        let comparisons = rows
            .iter()
            .filter(|r| r.value("id_airwork").is_some())
            .filter_map(|r| {
                Some(Comparison {
                    title: r.value("hs_name").unwrap_or("求人名未取得").into(),
                    publication: publication(r.value("baitai_genjoukyou_airwork"))?.into(),
                    body: r.value("shigotonaiyou").unwrap_or("").into(),
                })
            })
            .collect();
        let location = [record.value("todoufuken"), record.value("shikuchouson")]
            .into_iter()
            .flatten()
            .collect::<String>();
        Ok(Source {
            row,
            body: body.into(),
            comparisons,
            location: (!location.is_empty()).then_some(location),
        })
    }
}
pub(super) async fn generate(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let threshold = draft::threshold(std::env::var("RIKUROZI_OVERLAP_THRESHOLD").ok().as_deref())
        .map_err(|_| {
        ReadError(
            StatusCode::SERVICE_UNAVAILABLE,
            "draft_configuration_invalid",
        )
    })?;
    let service = access.service.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    let source = service.rikurozi_source(&id).await?;
    let result = draft::run(
        &source,
        threshold,
        |prompt, schema, temperature| async move {
            crate::job_gen::handlers::jobgen_llm(&prompt, &schema, temperature).await
        },
    )
    .await
    .map_err(|_| fail("draft_generation_failed"))?;
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_uses_only_media_status_and_includes_unknown() {
        for value in [Some("公開"), Some("掲載中")] {
            assert_eq!(publication(value), Some("掲載中"));
        }
        for value in [
            Some("非公開"),
            Some("公開終了"),
            Some("掲載終了"),
            Some("公開開始前"),
        ] {
            assert_eq!(publication(value), None);
        }
        for value in [None, Some(""), Some("不明"), Some("active"), Some("募集中")] {
            assert_eq!(publication(value), Some("掲載中か不明"));
        }
    }
    #[tokio::test]
    async fn collects_all_company_deals_deduplicates_and_paginates_without_other_customer() {
        use axum::{
            body::{to_bytes, Body},
            http::Request,
            routing::any,
        };
        let app = Router::new().fallback(any(|req: Request<Body>| async move {
            let path = req.uri().path().to_owned(); let query = req.uri().query().unwrap_or("").to_owned();
            let body = to_bytes(req.into_body(), 100000).await.unwrap();
            let body: Value = if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).unwrap() };
            let result = match path.as_str() {
                "/crm/v4/objects/0-420/1/associations/deals" => json!({"results":[{"toObjectId":10}]}),
                "/crm/v4/associations/deals/companies/batch/read" => json!({"results":[{"from":{"id":"10"},"to":[{"toObjectId":100}]}]}),
                "/crm/v4/objects/companies/100/associations/deals" if !query.contains("after=") => json!({"results":[{"toObjectId":10}],"paging":{"next":{"after":"10"}}}),
                "/crm/v4/objects/companies/100/associations/deals" => json!({"results":[{"toObjectId":20}]}),
                "/crm/v4/associations/deals/0-420/batch/read" => {
                    assert_eq!(body["inputs"],json!([{"id":"10"},{"id":"20"}]));
                    json!({"results":[{"from":{"id":"10"},"to":[{"toObjectId":1},{"toObjectId":2}]},{"from":{"id":"20"},"to":[{"toObjectId":2},{"toObjectId":3},{"toObjectId":4},{"toObjectId":5}]}]})
                },
                "/crm/v3/objects/0-420/batch/read" => {
                    let ids: Vec<_> = body["inputs"].as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
                    assert!(!ids.contains(&"99")); // Other company is never requested.
                    let rows: Vec<_> = ids.iter().map(|id| {
                        let properties = match *id {
                            "1" => json!({"id_hrhakkaa":"1234567","id_shop_hrhakkaa":"4081","hrh_kyuujinhyou_honbun":"案件名：基準の求人\n仕事内容：品物を集めます\n雇用形態：パート\n給与形態：時給\n基本給与 最小：1200\n勤務時間：9:00〜18:00\n自由項目1の内容：土日休み", "hrh_kyuujinhyou_gazou":"画像1：https://example.invalid/one.jpg\n画像2：なし\n画像3：https://example.invalid/three.png"}),
                            "2" => json!({"hs_name":"公開の求人","id_airwork":"77","shigotonaiyou":"棚卸しします","baitai_genjoukyou_airwork":"公開"}),
                            "3" => json!({"hs_name":"公開状況不明の求人","id_airwork":"77","shigotonaiyou":"箱を運びます"}),
                            "4" => json!({"hs_name":"掲載終了の求人","id_airwork":"78","baitai_genjoukyou_airwork":"掲載終了"}),
                            "5" => json!({"hs_name":"別のHRハッカー","id_hrhakkaa":"98765"}),
                            _ => panic!("unexpected id"),
                        }; json!({"id":id,"properties":properties})
                    }).collect(); json!({"results":rows})
                },
                _ => panic!("unexpected path {path}"),
            };
            Json(result)
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service =
            JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let source = service.rikurozi_source("1").await.unwrap();
        assert_eq!(source.row["求人id"], "1234567");
        assert_eq!(source.row["店舗id"], "4081");
        assert_eq!(source.row["画像1"], "https://example.invalid/one.jpg");
        assert_eq!(source.row["画像2"], "");
        assert_eq!(source.row["画像3"], "https://example.invalid/three.png");
        assert_eq!(source.comparisons.len(), 2);
        assert_eq!(source.comparisons[0].title, "公開の求人");
        assert_eq!(source.comparisons[1].publication, "掲載中か不明");
        assert_eq!(source.comparisons[1].body, "箱を運びます");
        task.abort();
    }
}
