//! Light pages and complete summaries, including reusable ended-listing body values.
use super::*;

pub(super) fn index_properties() -> Vec<&'static str> {
    PROPERTIES
        .iter()
        .copied()
        .filter(|p| {
            ![
                "hrh_kyuujinhyou_honbun",
                "shigotonaiyou",
                "hrh_kyuujinhyou_gazou",
                AW_IMAGES,
            ]
            .contains(p)
        })
        .collect()
}

// The list API has no body-specific revision in the existing property contract.
// hs_lastmodifieddate also changes for weekly results and cannot identify body edits.
// Explicit assumption: a listing's body does not change after publication has ended.
// Reuse only if BOTH generations are ended, the media binding is unchanged, and a
// body was actually acquired. Newly ended, reopened and unacquired rows must read.
fn reuse_ended_summary(row: &mut Listing, previous: Option<&Listing>) -> bool {
    let Some(old) = previous.filter(|old| {
        row.publication_status.as_deref() == Some("掲載終了")
            && old.publication_status.as_deref() == Some("掲載終了")
            && row.media == old.media
            && row.media_job_id == old.media_job_id
            && row.account_id == old.account_id
    }) else {
        return false;
    };
    let Some(body) = &old.body_summary else {
        return false;
    };
    // Keep current structured properties (location and AirWork columns) from the page.
    row.summary.salary_min = body.salary_min.clone();
    row.summary.salary_max = body.salary_max.clone();
    row.summary.body_changed_at = body.body_changed_at.clone();
    if row.media == "hrh" {
        row.summary.occupation = body.occupation.clone();
        row.summary.salary_kind = body.salary_kind.clone();
        row.summary.hours = body.hours.clone();
        row.summary.holidays = body.holidays.clone();
    }
    row.body_summary = Some(row.summary.clone());
    true
}

pub(super) async fn enrich_index_summaries(
    reader: &IndexReader,
    rows: &mut [Listing],
    previous: &BTreeMap<&str, &Listing>,
) -> Result<(), ReadError> {
    let mut ids = Vec::new();
    for row in rows.iter_mut() {
        if !reuse_ended_summary(row, previous.get(row.id.as_str()).copied()) {
            ids.push(row.id.clone());
        }
    }
    let mut positions: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.clone(), i))
        .collect();
    for group in ids.chunks(400) {
        let mut tasks = tokio::task::JoinSet::new();
        for chunk in group.chunks(100) {
            let (reader, ids) = (reader.clone(), chunk.to_vec());
            tasks.spawn(async move {
                let mut properties = index_properties();
                properties.extend(["hrh_kyuujinhyou_honbun", "shigotonaiyou"]);
                // Weekly results were already read by the list; avoid reading them twice.
                properties.retain(|p| ![INDEED_WEEKLY_PERF, AIRWORK_WEEKLY_PERF].contains(p));
                let data = reader
                    .read(
                        "/crm/v3/objects/0-420/batch/read",
                        &[],
                        Some(&json!({
                            "properties": properties,
                            "inputs": ids.iter().map(|id| json!({"id":id})).collect::<Vec<_>>()
                        })),
                        &super::super::listing_retry::DELAYS[..1],
                    )
                    .await?;
                let page: Page = serde_json::from_value(data.clone())
                    .map_err(|_| fail("hubspot_invalid_response"))?;
                if data
                    .get("errors")
                    .is_some_and(|v| v.as_array().is_none_or(|v| !v.is_empty()))
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
            for record in result.map_err(|_| fail("hubspot_read_failed"))?? {
                let position = positions
                    .remove(&record.id)
                    .ok_or_else(|| fail("hubspot_partial_read"))?;
                let row = &mut rows[position];
                row.summary = summary(&record, row.media);
                row.body_summary = Some(row.summary.clone());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::any};

    fn record(id: &str, media: &str, status: &str) -> Record {
        serde_json::from_value(json!({"id":id,"properties":{
            if media == "hrh" {"id_hrhakkaa"} else {"id_airwork"}: "synthetic",
            "kyuujin_status":status,"hs_name":"合成求人","jobcopy_indeed_title":"看護師",
            "todoufuken":"東京都","shikuchouson":"新宿区","qinwude":"東京都新宿区",
            "zhizhong":"看護補助","kyuuyokeitai":"時給","kinmujikan":"9時〜17時","kyuujitsu":"日曜"
        }}))
        .unwrap()
    }

    #[tokio::test]
    async fn active_prepublication_unknown_and_ended_have_exact_summaries() {
        let originals = [
            record("1", "hrh", "公開中"),
            record("2", "hrh", "公開前"),
            record("3", "airwork", "公開中"),
            record("4", "hrh", "公開終了"),
            record("5", "airwork", "公開終了"),
            record("6", "hrh", "不明"),
        ];
        let mut full = originals.clone();
        let body = "Indeed表示職種名：看護補助\n給与形態：時給\n基本給与 最小：0\n基本給与 最大：1800\n勤務時間：9時〜17時\n自由項目1のタイトル：休日・休暇\n自由項目1の内容：日曜";
        for r in &mut full {
            r.properties
                .insert("hrh_kyuujinhyou_honbun".into(), Some(body.into()));
            r.properties
                .insert("shigotonaiyou".into(), Some(body.into()));
        }
        let expected = full.clone();
        let app = Router::new().fallback(any(move |req: Request<Body>| {
            let full = full.clone();
            async move {
                assert_eq!(req.uri().path(), "/crm/v3/objects/0-420/batch/read");
                let bytes = axum::body::to_bytes(req.into_body(), 100000).await.unwrap();
                let input: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    input["inputs"],
                    json!([{"id":"1"},{"id":"2"},{"id":"3"},{"id":"4"},{"id":"5"},{"id":"6"}])
                );
                let results = full;
                Json(json!({"results":results}))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service =
            JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut rows: Vec<_> = originals.iter().map(|r| listing(r).unwrap()).collect();
        enrich_index_summaries(
            &IndexReader::new(&service.hs.background()),
            &mut rows,
            &BTreeMap::new(),
        )
        .await
        .unwrap();
        for i in 0..6 {
            assert_eq!(
                serde_json::to_value(&rows[i].summary).unwrap(),
                serde_json::to_value(summary(&expected[i], rows[i].media)).unwrap()
            );
            assert_eq!(rows[i].summary.occupation.as_deref(), Some("看護補助"));
            assert_eq!(rows[i].summary.salary_kind.as_deref(), Some("時給"));
            assert_eq!(rows[i].summary.salary_min.as_deref(), Some("0"));
            assert_eq!(rows[i].summary.salary_max.as_deref(), Some("1800"));
            assert_eq!(rows[i].summary.hours.as_deref(), Some("9時〜17時"));
            assert_eq!(rows[i].summary.holidays.as_deref(), Some("日曜"));
        }
        for row in &rows {
            assert!(matches(
                row,
                &ListingsQuery {
                    q: Some("合成".into()),
                    title: Some("看護師".into()),
                    ..Default::default()
                }
            ));
            assert!(!matches(
                row,
                &ListingsQuery {
                    q: Some("看護補助".into()),
                    ..Default::default()
                }
            ));
        }
        let mut old_rows: Vec<_> = expected.iter().map(|r| listing(r).unwrap()).collect();
        for row in old_rows.iter_mut().chain(rows.iter_mut()) {
            row.deal_ids = vec!["901".into()];
            row.perf = Some(super::super::home::PerfDigest {
                comparison_recent: None,
                latest_end: chrono::NaiveDate::from_ymd_opt(2026, 10, 11).unwrap(),
                recent_apps: 5,
                prev_apps: 3,
                recent_cost: 12000,
            });
        }
        let mut map = super::super::home::CompanyMap::default();
        map.deal_company.insert("901".into(), "701".into());
        map.names.insert("701".into(), Some("合成事業所".into()));
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        let home = super::super::home::compute(&rows, &map, today);
        assert_eq!(super::super::home::compute(&old_rows, &map, today), home);
        assert_eq!(home.customers.len(), 1);
        assert_eq!(home.customers[0].total_listings, 6);
        assert_eq!(home.customers[0].live_listings, 2);
        assert_eq!(home.customers[0].recent_apps, 10);
        assert_eq!(home.customers[0].prev_apps, 6);
        assert_eq!(home.customers[0].recent_cost, 24000);
        assert_eq!(
            home.cards.iter().find(|c| c.key == "more").unwrap().count,
            2
        );
        for publication in [None, Some("掲載中"), Some("掲載終了"), Some("掲載前")] {
            let query = ListingsQuery {
                publication: publication.map(str::to_owned),
                ..Default::default()
            };
            assert_eq!(
                old_rows
                    .iter()
                    .filter(|r| matches(r, &query))
                    .map(|r| &r.id)
                    .collect::<Vec<_>>(),
                rows.iter()
                    .filter(|r| matches(r, &query))
                    .map(|r| &r.id)
                    .collect::<Vec<_>>()
            );
        }
        task.abort();
    }

    #[tokio::test]
    async fn partial_duplicate_foreign_and_error_batches_are_rejected() {
        for response in [
            json!({"results":[]}),
            json!({"results":[{"id":"1","properties":{}},{"id":"1","properties":{}}]}),
            json!({"results":[{"id":"99","properties":{}}]}),
            json!({"results":[{"id":"1","properties":{}}],"errors":[{"status":"error"}]}),
        ] {
            let app = Router::new().fallback(any(move || {
                let response = response.clone();
                async move { Json(response) }
            }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let service =
                JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
            let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let mut rows = vec![listing(&record("1", "hrh", "公開中")).unwrap()];
            assert_eq!(
                enrich_index_summaries(
                    &IndexReader::new(&service.hs.background()),
                    &mut rows,
                    &BTreeMap::new()
                )
                .await
                .err()
                .unwrap()
                .1,
                "hubspot_partial_read"
            );
            task.abort();
        }
    }

    #[test]
    fn index_projection_keeps_weekly_results_but_omits_bodies_and_images() {
        let props = index_properties();
        assert!(props.contains(&INDEED_WEEKLY_PERF));
        for p in [
            "hrh_kyuujinhyou_honbun",
            "shigotonaiyou",
            "hrh_kyuujinhyou_gazou",
            AW_IMAGES,
        ] {
            assert!(!props.contains(&p));
        }
    }
}

#[cfg(test)]
#[path = "listing_index_reuse_tests.rs"]
mod reuse_tests;
