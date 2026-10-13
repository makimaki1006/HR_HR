//! Concrete summary and batch paging tests, using local mock servers only.
use super::*;
#[test]
fn job_copy_deal_salary_sort_matches_display_normalization() {
    for salary in [
        "250000 ",
        " 250000",
        "　２５０，０００　",
        "250,000\t",
        "250000.5 ",
    ] {
        let record: Record =
            serde_json::from_value(json!({"id":"1","properties":{"id_hrhakkaa":"H1"}})).unwrap();
        let mut first = listing(&record).unwrap();
        first.summary.salary_min = Some(salary.into());
        first.summary.salary_max = Some(salary.into());
        let mut second = first.clone();
        second.id = "2".into();
        second.summary.salary_min = Some("9000".into());
        second.summary.salary_max = Some("9000".into());
        let index = Index {
            records: vec![first, second],
            deals: vec![],
            titles: vec![],
            built_at: "2026-10-11T00:00:00Z".into(),
        };
        for sort in ["salary_min", "salary_max"] {
            for descending in [false, true] {
                let (_, rows) = selected_page(
                    &index,
                    &ListingsQuery {
                        sort: Some(sort.into()),
                        descending: Some(descending),
                        ..Default::default()
                    },
                );
                assert_eq!(
                    rows.iter()
                        .map(|row| row.listing.id.as_str())
                        .collect::<Vec<_>>(),
                    if descending {
                        vec!["1", "2"]
                    } else {
                        vec!["2", "1"]
                    },
                    "{salary:?} {sort} descending={descending}"
                );
            }
        }
    }
}
#[test]
fn job_copy_deal_summary_sort_and_body_date_are_explicit_columns() {
    let record: Record = serde_json::from_value(json!({"id":"1","properties":{"id_hrhakkaa":"H1","todoufuken":"沖縄県","shikuchouson":"那覇市","hrh_kyuujinhyou_honbun":"仕事内容：月給99万円で日曜日が休みです\nIndeed表示職種名：配送ドライバー\n給与形態：月給\n基本給与 最小：250000\n基本給与 最大：300000\n勤務時間：8:00〜17:00\n自由項目1のタイトル：休日・休暇\n自由項目1の内容：土日休み"}})).unwrap();
    let mut first = listing(&record).unwrap();
    assert_eq!(first.summary.salary_kind.as_deref(), Some("月給"));
    assert_eq!(first.summary.salary_min.as_deref(), Some("250000"));
    assert_eq!(first.summary.salary_max.as_deref(), Some("300000"));
    assert_eq!(first.summary.holidays.as_deref(), Some("土日休み"));
    assert_eq!(first.summary.hours.as_deref(), Some("8:00〜17:00"));
    assert_eq!(first.summary.location.as_deref(), Some("沖縄県那覇市"));
    first.indexed_application_count = Some(2);
    let mut second = first.clone();
    second.id = "2".into();
    second.summary.salary_min = Some("9000".into());
    second.indexed_application_count = Some(10);
    let index = Index {
        records: vec![first, second],
        deals: vec![],
        titles: vec![],
        built_at: "2026-10-11T00:00:00Z".into(),
    };
    assert_eq!(
        selected_page(
            &index,
            &ListingsQuery {
                sort: Some("salary_min".into()),
                ..Default::default()
            }
        )
        .1
        .iter()
        .map(|v| v.listing.id.as_str())
        .collect::<Vec<_>>(),
        vec!["2", "1"]
    );
    assert_eq!(
        selected_page(
            &index,
            &ListingsQuery {
                sort: Some("applications".into()),
                descending: Some(true),
                ..Default::default()
            }
        )
        .1
        .iter()
        .map(|v| v.listing.id.as_str())
        .collect::<Vec<_>>(),
        vec!["2", "1"]
    );
    let plain: Record = serde_json::from_value(json!({"id":"3","properties":{"id_airwork":"A3","shigotonaiyou":"月給25万円。土日休み。8時から17時です"}})).unwrap();
    let summary = summary(&plain, "airwork");
    assert!(summary.salary_min.is_none());
    assert!(summary.holidays.is_none());
    assert!(summary.hours.is_none());
    assert_eq!(publication(&plain, "airwork").as_deref(), Some("不明"));
    let data = json!({"properties":{"hrh_kyuujinhyou_honbun":"現在"},"propertiesWithHistory":{"hrh_kyuujinhyou_honbun":[{"timestamp":"2026-10-11T00:00:00Z","value":"現在"},{"timestamp":"2026-10-10T00:00:00Z","value":"現在"},{"timestamp":"2026-10-09T00:00:00Z","value":"以前"}]}});
    assert_eq!(
        body_changed_at(&data, "hrh").as_deref(),
        Some("2026-10-10T00:00:00+00:00")
    );
    assert_eq!(
        body_changed_at(&json!({"updatedAt":"2026-10-11T00:00:00Z"}), "hrh"),
        None
    );
}
#[tokio::test]
async fn job_copy_deal_batch_pages_are_complete_and_missing_rows_fail() {
    use axum::{body::Body, http::Request, routing::any};
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = calls.clone();
    let server = Router::new().fallback(any(move |request: Request<Body>| {
            let seen = seen.clone();
            async move {
                assert_eq!(request.uri().path(),"/crm/v4/associations/0-420/deals/batch/read");
                let input: Value = serde_json::from_slice(&axum::body::to_bytes(request.into_body(), 100000).await.unwrap()).unwrap();
                seen.lock().unwrap().push(input.clone());
                let rows: Vec<_> = input["inputs"].as_array().unwrap().iter().filter(|v| v["id"] != "9999").map(|v| {
                    if v["id"] == "1" && v.get("after").is_none() { json!({"from":{"id":"1"},"to":[{"toObjectId":91}],"paging":{"next":{"after":"page2"}}}) }
                    else if v["id"] == "1" { json!({"from":{"id":"1"},"to":[{"toObjectId":91},{"toObjectId":92}]}) }
                    else { json!({"from":{"id":v["id"]},"to":[]}) }
                }).collect();
                Json(json!({"results":rows}))
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let task = tokio::spawn(async move {
        axum::serve(listener, server).await.unwrap();
    });
    let ids: Vec<_> = (1..=1001).map(|v| v.to_string()).collect();
    let mapping = index_associations(&service, &service.hs.background(), &ids, "deals")
        .await
        .unwrap();
    assert_eq!(mapping.len(), 1001);
    assert_eq!(mapping["1"], vec!["91", "92"]);
    assert!(mapping["1001"].is_empty());
    let requests = calls.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0]["inputs"].as_array().unwrap().len(), 1000);
    assert_eq!(requests[1], json!({"inputs":[{"id":"1","after":"page2"}]}));
    assert_eq!(requests[2], json!({"inputs":[{"id":"1001"}]}));
    assert!(index_associations(
        &service,
        &service.hs.background(),
        &["9999".into()],
        "deals"
    )
    .await
    .is_err());
    task.abort();
}

#[tokio::test]
async fn job_copy_visible_history_reads_are_batches_of_fifty_and_exclude_other_rows() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
        routing::any,
    };
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = calls.clone();
    let app = Router::new().fallback(any(move |req: Request<Body>| { let seen = seen.clone(); async move {
        assert_eq!(req.uri().path(), "/crm/v3/objects/0-420/batch/read");
        let input: Value = serde_json::from_slice(&to_bytes(req.into_body(),100000).await.unwrap()).unwrap();
        let ids = input["inputs"].as_array().unwrap();
        assert!(ids.len() <= 50);
        assert_eq!(input["propertiesWithHistory"], json!(["hrh_kyuujinhyou_honbun","shigotonaiyou"]));
        seen.lock().unwrap().push(ids.clone());
        Json(json!({"status":"COMPLETE","startedAt":"2026-10-11T00:00:00Z","completedAt":"2026-10-11T00:00:01Z","results":ids.iter().map(|v| json!({"id":v["id"],"properties":{"id_airwork":"123","shigotonaiyou":"架空の本文"},"propertiesWithHistory":{"shigotonaiyou":[{"value":"架空の本文","timestamp":"2026-10-10T00:00:00Z","sourceType":"API","sourceId":"synthetic"}]},"createdAt":"2026-10-01T00:00:00Z","updatedAt":"2026-10-11T00:00:00Z","archived":false})).collect::<Vec<_>>() }))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let ids: Vec<_> = (51..=151).map(|n| n.to_string()).collect();
    let result = deals::body_dates(&service, &ids).await.unwrap();
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![50, 50, 1]
    );
    assert_eq!(result["dates"].as_object().unwrap().len(), 101);
    assert_eq!(result["dates"]["51"], "2026-10-10T00:00:00+00:00");
    assert!(result["dates"].get("1").is_none());
    task.abort();
}
#[test]
fn job_copy_airwork_real_response_shape_maps_independent_fields_without_salary_inference() {
    let record: Record = serde_json::from_value(json!({"id":"1","properties":{"hs_name":"架空の配送スタッフ","id_airwork":"123","shigotonaiyou":"箱を積み込んで届けます。","kyuuyokeitai":"月給","qinwude":"東京都江東区","zhizhong":"配送ドライバー","kinmujikan":"8:00〜17:00","kyuujitsu":null,"guyongxingtai":null,"hrh_kyuujinhyou_honbun":null}})).unwrap();
    let row = listing(&record).unwrap();
    assert_eq!(row.summary.salary_kind.as_deref(), Some("月給"));
    assert_eq!(row.summary.salary_min, None);
    assert_eq!(row.summary.location.as_deref(), Some("東京都江東区"));
    assert_eq!(row.summary.hours.as_deref(), Some("8:00〜17:00"));
    let fields = deals::comparison_fields(&record, "airwork");
    assert_eq!(fields["仕事内容"], "箱を積み込んで届けます。");
    assert_eq!(fields["給与形態"], "月給");
    assert_eq!(fields["勤務地"], "東京都江東区");
    assert!(!fields.contains_key("基本給与 最小"));
    assert!(!fields.contains_key("雇用形態"));
}
#[test]
fn job_copy_holidays_can_be_work_supplement_two_instead_of_free_one() {
    let record: Record = serde_json::from_value(json!({"id":"1","properties":{"id_hrhakkaa":"123","hrh_kyuujinhyou_honbun":"仕事内容：棚の品物を集めます\n仕事情報補足2のタイトル：休日休暇\n仕事情報補足2の内容：週休2日・夏季休暇\n自由項目1のタイトル：応募案内\n自由項目1の内容：電話でご連絡ください"}})).unwrap();
    assert_eq!(
        summary(&record, "hrh").holidays.as_deref(),
        Some("週休2日・夏季休暇")
    );
    assert_eq!(
        deals::comparison_fields(&record, "hrh")["休日・休暇"],
        "週休2日・夏季休暇"
    );
}
