//! Rebuilds with/without a real fake-Turso round trip preserve complete body values.
use super::*;
use axum::{body::Body, http::Request, routing::any};

fn full(id: &str, media: &str, status: &str, amount: &str) -> Record {
    let body = format!("Indeed表示職種名：看護補助\n給与形態：時給\n基本給与 最小：{amount}\n基本給与 最大：1800\n勤務時間：9時〜17時\n自由項目1のタイトル：休日・休暇\n自由項目1の内容：日曜");
    serde_json::from_value(json!({"id":id,"properties":{
        if media == "hrh" {"id_hrhakkaa"} else {"id_airwork"}: format!("synthetic-{id}"),
        "kyuujin_status":status,"hs_name":"合成求人","jobcopy_indeed_title":"看護師",
        "todoufuken":"東京都","shikuchouson":"新宿区","qinwude":"東京都新宿区",
        "zhizhong":"看護補助","kyuuyokeitai":"時給","kinmujikan":"9時〜17時","kyuujitsu":"日曜",
        "hrh_kyuujinhyou_honbun":body,"shigotonaiyou":body
    }}))
    .unwrap()
}
fn light(record: &Record) -> Listing {
    let mut record = record.clone();
    record.properties.remove("hrh_kyuujinhyou_honbun");
    record.properties.remove("shigotonaiyou");
    listing(&record).unwrap()
}
fn assert_values(row: &Listing, record: &Record, amount: &str) {
    assert_eq!(
        serde_json::to_value(&row.summary).unwrap(),
        serde_json::to_value(summary(record, row.media)).unwrap()
    );
    assert_eq!(row.summary.occupation.as_deref(), Some("看護補助"));
    assert_eq!(row.summary.salary_kind.as_deref(), Some("時給"));
    assert_eq!(row.summary.salary_min.as_deref(), Some(amount));
    assert_eq!(row.summary.salary_max.as_deref(), Some("1800"));
    assert_eq!(row.summary.hours.as_deref(), Some("9時〜17時"));
    assert_eq!(row.summary.holidays.as_deref(), Some("日曜"));
    assert!(row.body_summary.is_some());
    // The private acquisition marker is persisted but never sent to browsers.
    assert!(serde_json::to_value(row)
        .unwrap()
        .get("body_summary")
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_save_restored_newly_ended_and_reopened_match_full_main_summaries() {
    let first: Vec<_> = ["hrh", "airwork"]
        .into_iter()
        .enumerate()
        .flat_map(|(m, media)| {
            ["公開終了", "公開中", "公開終了", "公開前", "不明"]
                .into_iter()
                .enumerate()
                .map(move |(i, status)| full(&(m * 5 + i + 1).to_string(), media, status, "0"))
        })
        .collect();
    let current = Arc::new(std::sync::Mutex::new(first.clone()));
    let calls = Arc::new(std::sync::Mutex::new(Vec::<Vec<String>>::new()));
    let (records, seen) = (current.clone(), calls.clone());
    let app = Router::new().fallback(any(move |req: Request<Body>| {
        let (records, seen) = (records.clone(), seen.clone());
        async move {
            assert_eq!(req.uri().path(), "/crm/v3/objects/0-420/batch/read");
            let bytes = axum::body::to_bytes(req.into_body(), 100000).await.unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            let ids: Vec<_> = body["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["id"].as_str().unwrap().to_owned())
                .collect();
            assert!(ids.len() <= 100);
            seen.lock().unwrap().push(ids.clone());
            let rows: Vec<_> = records
                .lock()
                .unwrap()
                .iter()
                .filter(|r| ids.contains(&r.id))
                .cloned()
                .collect();
            Json(json!({"results":rows}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let reader = IndexReader::new(&service.hs.background());
    let mut initial: Vec<_> = first.iter().map(light).collect();
    // No save: even ended listings acquire their complete values, including real zero.
    enrich_index_summaries(&reader, &mut initial, &BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        vec![first.iter().map(|r| r.id.clone()).collect::<Vec<_>>()]
    );
    for (row, record) in initial.iter().zip(&first) {
        assert_values(row, record, "0");
    }
    let index = Index {
        records: initial,
        titles: vec![],
        deals: vec![],
        built_at: chrono::Utc::now().to_rfc3339(),
    };
    let restored = super::super::snapshot::test_round_trip(index.clone()).await;
    for previous in [&index, &restored] {
        let next: Vec<_> = first
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let status = match i % 5 {
                    0 => "公開終了",
                    1 => "公開終了",
                    2 => "公開中",
                    3 => "公開前",
                    _ => "不明",
                };
                full(
                    &r.id,
                    if i < 5 { "hrh" } else { "airwork" },
                    status,
                    if i % 5 == 0 { "0" } else { "1500" },
                )
            })
            .chain([
                full("11", "hrh", "公開終了", "1500"),
                full("12", "airwork", "公開終了", "1500"),
            ])
            .collect();
        *current.lock().unwrap() = next.clone();
        calls.lock().unwrap().clear();
        let previous: BTreeMap<_, _> = previous
            .records
            .iter()
            .map(|r| (r.id.as_str(), r))
            .collect();
        let mut rows: Vec<_> = next.iter().map(light).collect();
        enrich_index_summaries(&reader, &mut rows, &previous)
            .await
            .unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            vec![vec!["2", "3", "4", "5", "7", "8", "9", "10", "11", "12"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()]
        );
        for (i, (row, record)) in rows.iter().zip(&next).enumerate() {
            assert_values(row, record, if i < 10 && i % 5 == 0 { "0" } else { "1500" });
        }
    }
    server.abort();
}

#[test]
fn acquired_empty_values_are_reusable_but_missing_provenance_and_binding_changes_are_not() {
    let record = full("1", "hrh", "公開終了", "0");
    let mut old = light(&record);
    let mut row = light(&record);
    assert!(!reuse_ended_summary(&mut row, Some(&old)));
    old.body_summary = Some(Summary::default());
    assert!(reuse_ended_summary(&mut row, Some(&old)));
    assert!(row.summary.salary_min.is_none());
    row.media_job_id = "new-binding".into();
    assert!(!reuse_ended_summary(&mut row, Some(&old)));
}

#[test]
fn reused_body_values_keep_fresh_location_and_airwork_structured_columns() {
    let record = full("1", "airwork", "公開終了", "0");
    let mut old = light(&record);
    old.body_summary = Some(summary(&record, "airwork"));
    let mut current = record.clone();
    for (key, value) in [
        ("qinwude", "大阪府大阪市"),
        ("zhizhong", "配送"),
        ("kyuuyokeitai", "月給"),
        ("kinmujikan", "10時〜18時"),
        ("kyuujitsu", "土曜"),
    ] {
        current.properties.insert(key.into(), Some(value.into()));
    }
    let mut row = light(&current);
    assert!(reuse_ended_summary(&mut row, Some(&old)));
    assert_eq!(
        serde_json::to_value(&row.summary).unwrap(),
        serde_json::to_value(summary(&current, "airwork")).unwrap()
    );
    assert_eq!(row.summary.location.as_deref(), Some("大阪府大阪市"));
    assert_eq!(row.summary.hours.as_deref(), Some("10時〜18時"));
    assert_eq!(row.summary.salary_min.as_deref(), Some("0"));
}
