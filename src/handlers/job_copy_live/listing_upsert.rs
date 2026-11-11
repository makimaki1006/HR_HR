//! 1 件の求人を一覧の索引に足す・差し替える（6 時間ごとの作り直しを待たない）。
//!
//! 使う場面: 「この案でCSVを出す」でリクロジ向けの求人を作ったとき、リクロジの求人IDを入れたとき。
//! 求人 1 件を HubSpot から読み直し、作り直しと同じ `listing()` で行を作る。媒体の決まり
//! (HRハッカーとリクロジの両方の ID を持つ求人は出さない) もそのまま当たり、行にならない求人は索引から外す。
//!
//! 作り直しの最中に足した分は、作り直しが終わった索引にも当て直す (作り直しは足す前に読んだ範囲を持つことがあるため)。
//! 保存 (ADR-029 の監査 Turso) は作り直しが終わったときだけで、ここでは書かない。
//! 再起動後は前回の保存から戻した索引に続いて全件の作り直しが走り、そこで HubSpot から読み直される。
use super::*;

/// 作り直しの間に覚えておく 1 件分。`row` が None なら索引から外す。
#[derive(Clone)]
pub(super) struct Upsert {
    row: Option<Listing>,
    /// 索引にまだ無かった、納品管理の取引。
    deals: Vec<Deal>,
}

/// 索引の 1 件を差し替える。取引ごとの求人数も合わせ、0 件になった取引は外す (作り直しと同じ形)。
pub(super) fn apply(index: &mut Index, id: &str, update: &Upsert) {
    let position = index.records.iter().position(|r| r.id == id);
    if let Some(position) = position {
        let old = index.records.remove(position);
        for deal in &old.deal_ids {
            if let Some(d) = index.deals.iter_mut().find(|d| &d.id == deal) {
                d.count = d.count.saturating_sub(1);
            }
        }
    }
    for deal in &update.deals {
        if !index.deals.iter().any(|d| d.id == deal.id) {
            let mut deal = deal.clone();
            deal.count = 0;
            index.deals.push(deal);
        }
    }
    if let Some(row) = &update.row {
        let mut row = row.clone();
        row.deal_ids
            .retain(|deal| index.deals.iter().any(|d| &d.id == deal));
        for deal in &row.deal_ids {
            if let Some(d) = index.deals.iter_mut().find(|d| &d.id == deal) {
                d.count += 1;
            }
        }
        match position {
            Some(position) => index.records.insert(position, row),
            None => index.records.push(row),
        }
    }
    index.deals.retain(|d| d.count > 0);
    index.deals.sort_by(|a, b| a.id.cmp(&b.id));
}

/// 求人 1 件を HubSpot から読み直して索引に入れる。`linked_deals` は直前に関連を付けた取引
/// (関連の読み取りが追いつく前でも行に付ける)。
///
/// HubSpot の呼び出し: 求人 1 回 + 関連 2 回 (取引・応募) + 索引に無い取引があればその取引 1 回。
pub(in crate::handlers::job_copy_live) async fn refresh_listing(
    service: &JobReadService,
    id: &str,
    linked_deals: &[String],
) -> Result<(), ReadError> {
    let reader = IndexReader::new(&service.hs);
    let mut properties = index_properties();
    properties.extend(["hrh_kyuujinhyou_honbun", "shigotonaiyou"]);
    let data = reader
        .read(
            "/crm/v3/objects/0-420/batch/read",
            &[],
            Some(&json!({"properties": properties, "inputs": [{"id": id}]})),
            &super::super::listing_retry::DELAYS[..1],
        )
        .await?;
    let page: Page = serde_json::from_value(data).map_err(|_| fail("hubspot_invalid_response"))?;
    let record = page.results.into_iter().find(|r| r.id == id);
    let mut row = record.as_ref().and_then(listing);
    let mut new_deals = Vec::new();
    if let (Some(row), Some(record)) = (row.as_mut(), record.as_ref()) {
        row.summary = summary(record, row.media);
        row.body_summary = Some(row.summary.clone());
        let ids = [id.to_owned()];
        let mut deal_ids: BTreeSet<String> = index_associations(&reader, &ids, "deals")
            .await?
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .collect();
        deal_ids.extend(linked_deals.iter().cloned());
        row.indexed_application_count = Some(
            index_associations(&reader, &ids, "0-421")
                .await?
                .remove(id)
                .map_or(0, |v| v.len()),
        );
        row.deal_ids = deal_ids.into_iter().collect();
        let known: BTreeSet<String> = service
            .listing_index
            .state
            .read()
            .await
            .snapshot
            .as_ref()
            .map(|index| index.deals.iter().map(|d| d.id.clone()).collect())
            .unwrap_or_default();
        let unknown: Vec<String> = row
            .deal_ids
            .iter()
            .filter(|d| !known.contains(*d))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            for record in deals::index_deal_records(&reader, &unknown).await? {
                // 納品管理のパイプラインの取引だけを扱う (作り直しと同じ)。
                if pipelines::is_delivery(record.value("pipeline")) {
                    new_deals.push(Deal {
                        id: record.id.clone(),
                        name: record.value("dealname").map(str::to_owned),
                        pipeline: record.value("pipeline").map(str::to_owned),
                        stage: record.value("dealstage").map(str::to_owned),
                        created_at: record
                            .value("createdate")
                            .map(|v| v.chars().take(10).collect()),
                        assignment: owner_todo::Assignment::from_record(&record),
                        count: 0,
                    });
                }
            }
        }
    }
    let update = Upsert {
        row,
        deals: new_deals,
    };
    let mut state = service.listing_index.state.write().await;
    if state.refreshing {
        state.listing_updates.insert(id.to_owned(), update.clone());
    }
    if let Some(index) = state.snapshot.as_mut() {
        apply(Arc::make_mut(index), id, &update);
    }
    Ok(())
}

/// ほかのモジュールのテストから索引を用意・確かめるための口。
#[cfg(test)]
pub(in crate::handlers::job_copy_live) mod test_support {
    use super::*;
    /// 空の索引を置く (作り直しが一度済んだ状態)。
    pub(in crate::handlers::job_copy_live) async fn empty_index(service: &JobReadService) {
        service.listing_index.state.write().await.snapshot = Some(Arc::new(Index {
            records: vec![],
            titles: vec![],
            built_at: "2026-11-12T00:00:00Z".into(),
            deals: vec![],
        }));
    }
    /// 索引の行 (媒体, 媒体の求人ID, 掲載の状態, 取引)。
    pub(in crate::handlers::job_copy_live) async fn row(
        service: &JobReadService,
        id: &str,
    ) -> Option<(&'static str, String, Option<String>, Vec<String>)> {
        let state = service.listing_index.state.read().await;
        let row = state
            .snapshot
            .as_ref()?
            .records
            .iter()
            .find(|r| r.id == id)?;
        Some((
            row.media,
            row.media_job_id.clone(),
            row.publication_status.clone(),
            row.deal_ids.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request},
        routing::any,
        Router,
    };

    fn row(id: &str, deals: &[&str]) -> Listing {
        let mut row = listing(
            &serde_json::from_value(
                json!({"id":id,"properties":{"id_hrhakkaa":id,"hs_name":"既存"}}),
            )
            .unwrap(),
        )
        .unwrap();
        row.deal_ids = deals.iter().map(|d| (*d).to_owned()).collect();
        row
    }
    fn deal(id: &str, count: usize) -> Deal {
        Deal {
            id: id.into(),
            name: Some(format!("取引{id}")),
            pipeline: Some("p".into()),
            stage: None,
            created_at: None,
            assignment: Default::default(),
            count,
        }
    }
    fn index() -> Index {
        Index {
            records: vec![row("1", &["10"]), row("2", &["10", "20"])],
            titles: vec![],
            built_at: "2026-11-12T00:00:00Z".into(),
            deals: vec![deal("10", 2), deal("20", 1)],
        }
    }
    fn counts(index: &Index) -> Vec<(String, usize)> {
        index
            .deals
            .iter()
            .map(|d| (d.id.clone(), d.count))
            .collect()
    }

    #[test]
    fn apply_adds_replaces_and_removes_with_deal_counts() {
        let mut index = index();
        // 足す: 既存の取引 20 と、新しい取引 30。索引に無い取引 99 は付けない。
        let mut new = row("900", &["20", "30", "99"]);
        new.media = "reclogi";
        apply(
            &mut index,
            "900",
            &Upsert {
                row: Some(new),
                deals: vec![deal("30", 0)],
            },
        );
        assert_eq!(index.records.len(), 3);
        assert_eq!(index.records[2].deal_ids, ["20", "30"]);
        assert_eq!(
            counts(&index),
            [("10".into(), 2), ("20".into(), 2), ("30".into(), 1)]
        );
        // 差し替え: 位置を保ち、件数は二重に数えない。
        let mut replaced = row("900", &["30"]);
        replaced.media_job_id = "424242".into();
        apply(
            &mut index,
            "900",
            &Upsert {
                row: Some(replaced),
                deals: vec![],
            },
        );
        assert_eq!(index.records.len(), 3);
        assert_eq!(index.records[2].media_job_id, "424242");
        assert_eq!(
            counts(&index),
            [("10".into(), 2), ("20".into(), 1), ("30".into(), 1)]
        );
        // 行にならない求人は外す。求人が 0 件になった取引も外す。
        apply(
            &mut index,
            "900",
            &Upsert {
                row: None,
                deals: vec![],
            },
        );
        assert_eq!(index.records.len(), 2);
        assert_eq!(counts(&index), [("10".into(), 2), ("20".into(), 1)]);
    }

    /// 偽の HubSpot。求人 900 (リクロジ公開前)・取引 10 (索引に既存) と 30 (納品管理の新しい取引)・応募 2 件。
    async fn serve(listing_props: Value) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        let calls = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen = calls.clone();
        let app = Router::new().fallback(any(move |req: Request<Body>| {
            let (calls, props) = (seen.clone(), listing_props.clone());
            async move {
                let (method, path) = (req.method().clone(), req.uri().path().to_owned());
                let body: Value =
                    serde_json::from_slice(&to_bytes(req.into_body(), 1_000_000).await.unwrap())
                        .unwrap_or(Value::Null);
                calls.lock().unwrap().push(path.clone());
                let reply = match (method, path.as_str()) {
                    (Method::POST, "/crm/v3/objects/0-420/batch/read") => {
                        assert_eq!(body["inputs"], json!([{"id":"900"}]));
                        json!({"results":[{"id":"900","properties":props}]})
                    }
                    (Method::POST, "/crm/v4/associations/0-420/deals/batch/read") => {
                        json!({"results":[{"from":{"id":"900"},"to":[{"toObjectId":10},{"toObjectId":30}]}]})
                    }
                    (Method::POST, "/crm/v4/associations/0-420/0-421/batch/read") => {
                        json!({"results":[{"from":{"id":"900"},"to":[{"toObjectId":1},{"toObjectId":2}]}]})
                    }
                    (Method::POST, "/crm/v3/objects/deals/batch/read") => {
                        assert_eq!(body["inputs"], json!([{"id":"30"}]));
                        json!({"results":[{"id":"30","properties":{
                            "dealname":"新しい取引","pipeline":pipelines::DELIVERY_PIPELINES[0].0,
                            "dealstage":"s1","createdate":"2026-11-01T00:00:00Z"}}]})
                    }
                    other => panic!("unexpected request {other:?}"),
                };
                axum::Json(reply)
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (base, calls)
    }
    fn reclogi_props() -> Value {
        json!({"reclogi_moto_hr_kyuujin_id":"1234567","kyuujin_status":"公開前","hs_name":"新しい案件名",
               "qinwude":"東京都千代田区丸の内","zhizhong":"介護職"})
    }

    #[tokio::test]
    async fn created_rikurozi_listing_appears_as_prepublication_at_once() {
        let (base, calls) = serve(reclogi_props()).await;
        let service = JobReadService::for_test(base);
        service.listing_index.state.write().await.snapshot = Some(Arc::new(index()));
        refresh_listing(&service, "900", &["10".to_owned()])
            .await
            .unwrap();
        let state = service.listing_index.state.read().await;
        let index = state.snapshot.as_ref().unwrap();
        let row = index.records.iter().find(|r| r.id == "900").unwrap();
        assert_eq!(row.media, "reclogi");
        assert_eq!(row.media_job_id, "");
        assert_eq!(row.title.as_deref(), Some("新しい案件名"));
        assert_eq!(row.publication_status.as_deref(), Some("掲載前"));
        assert_eq!(row.prefecture.as_deref(), Some("東京都"));
        assert_eq!(row.deal_ids, ["10", "30"]);
        assert_eq!(row.indexed_application_count, Some(2));
        assert_eq!(
            counts(index),
            [("10".into(), 3), ("20".into(), 1), ("30".into(), 1)]
        );
        let new = index.deals.iter().find(|d| d.id == "30").unwrap();
        assert_eq!(new.name.as_deref(), Some("新しい取引"));
        assert_eq!(new.created_at.as_deref(), Some("2026-11-01"));
        assert_eq!(calls.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn reclogi_id_replaces_the_row_and_both_ids_remove_it() {
        let mut props = reclogi_props();
        props["id_reclogi"] = json!("424242");
        let (base, _) = serve(props).await;
        let service = JobReadService::for_test(base);
        let mut start = index();
        let mut pending = row("900", &["10"]);
        pending.media = "reclogi";
        pending.media_job_id = String::new();
        start.records.push(pending);
        start.deals[0].count = 3;
        start.deals.push(deal("30", 0));
        service.listing_index.state.write().await.snapshot = Some(Arc::new(start));
        refresh_listing(&service, "900", &[]).await.unwrap();
        {
            let state = service.listing_index.state.read().await;
            let index = state.snapshot.as_ref().unwrap();
            let rows: Vec<_> = index.records.iter().filter(|r| r.id == "900").collect();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].media_job_id, "424242");
            assert_eq!(rows[0].publication_status.as_deref(), Some("掲載前"));
            assert_eq!(
                counts(index),
                [("10".into(), 3), ("20".into(), 1), ("30".into(), 1)]
            );
        }
        // HRハッカーとリクロジの両方の ID を持つ求人は出さない (決まりは変えない)。
        let mut both = reclogi_props();
        both["id_reclogi"] = json!("424242");
        both["id_hrhakkaa"] = json!("1234567");
        let (base, calls) = serve(both).await;
        let mut other = JobReadService::for_test(base);
        other.listing_index = service.listing_index.clone();
        refresh_listing(&other, "900", &[]).await.unwrap();
        let state = service.listing_index.state.read().await;
        let index = state.snapshot.as_ref().unwrap();
        assert!(index.records.iter().all(|r| r.id != "900"));
        assert_eq!(counts(index), [("10".into(), 2), ("20".into(), 1)]);
        // 行にならない求人は関連を読まない。
        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_update_during_a_rebuild_is_applied_to_the_rebuilt_index() {
        let (base, _) = serve(reclogi_props()).await;
        let service = JobReadService::for_test(base);
        {
            let mut state = service.listing_index.state.write().await;
            state.snapshot = Some(Arc::new(index()));
            state.refreshing = true;
        }
        refresh_listing(&service, "900", &[]).await.unwrap();
        let mut state = service.listing_index.state.write().await;
        let update = state.listing_updates.get("900").cloned().unwrap();
        // 作り直しで読んだ索引 (900 を含まない) に当て直すと、900 が入る。
        let mut rebuilt = index();
        apply(&mut rebuilt, "900", &update);
        assert!(rebuilt.records.iter().any(|r| r.id == "900"));
        state.listing_updates.clear();
    }
}
