use super::*;

fn fixture(count: usize) -> Index {
    let mut records = Vec::new();
    for n in 0..count {
        let mut row: Listing = serde_json::from_value(json!({
            "id":(n+100).to_string(), "media":if n%2==0 {"hrh"} else {"airwork"},
            "media_job_id":format!("{}", n+900000), "account_id":format!("{}", n%500),
            "title":format!("合成求人{}・配送と倉庫作業", n), "prefecture":"東京都", "municipality":"新宿区",
            "category":"配送", "indeed_title":"配送", "indeed_title_state":"set",
            "publication_status":"公開中", "draft_status":"adopted", "last_csv_detected_at":"2026-10-15",
            "deal_ids":[(n%500+800000).to_string()], "summary":{
                "occupation":"配送", "location":"東京都新宿区", "salary_kind":"月給",
                "salary_min":(200000+n%10000).to_string(), "salary_max":"350000", "hours":"9:00〜18:00", "holidays":"週休2日", "body_changed_at":"2026-10-15T01:00:00Z"
            }
        })).unwrap();
        row.indexed_application_count = Some(n % 31);
        row.body_summary = Some(row.summary.clone());
        row.perf = Some(home::PerfDigest {
            latest_end: chrono::NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
            recent_apps: (n % 20) as i64,
            recent_cost: (n % 10000) as i64,
            prev_apps: 3,
        });
        records.push(row);
    }
    Index {
        records,
        titles: vec!["配送".into()],
        built_at: chrono::Utc::now().to_rfc3339(),
        deals: (0..500)
            .map(|n| Deal {
                id: (n + 800000).to_string(),
                name: Some(format!("合成取引{n}")),
                pipeline: Some("delivery".into()),
                stage: Some("active".into()),
                created_at: Some("2026-10-01".into()),
                count: count / 500,
            })
            .collect(),
    }
}
fn assert_same(actual: &Index, expected: &Index) {
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    for (a, b) in actual.records.iter().zip(&expected.records) {
        assert_eq!(a.indexed_application_count, b.indexed_application_count);
        assert_eq!(a.perf, b.perf);
        assert_eq!(
            serde_json::to_value(&a.body_summary).unwrap(),
            serde_json::to_value(&b.body_summary).unwrap()
        );
    }
    for (a, b) in actual.deals.iter().zip(&expected.deals) {
        assert_eq!(
            (&a.pipeline, &a.stage, &a.created_at),
            (&b.pipeline, &b.stage, &b.created_at)
        );
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn round_trip_and_reject_invalid_snapshots() {
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    tokio::task::spawn_blocking(move || {
        let index = fixture(12);
        save(&audit, &index).unwrap();
        assert_same(&load(&audit).unwrap().unwrap(), &index);
        conn.lock()
            .unwrap()
            .execute("UPDATE job_copy_index_snapshots SET version=1", [])
            .unwrap();
        assert!(load(&audit).unwrap().is_none());
        save(&audit, &index).unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE job_copy_index_snapshots SET built_at='2000-01-01T00:00:00Z'",
                [],
            )
            .unwrap();
        assert!(load(&audit).unwrap().is_none());
        save(&audit, &index).unwrap();
        conn.lock()
            .unwrap()
            .execute("UPDATE job_copy_index_snapshots SET payload='broken'", [])
            .unwrap();
        assert!(load(&audit).is_err());
        let (bytes, _) = encode(&index).unwrap();
        let mut value: Value = serde_json::from_slice(&{
            let mut data = Vec::new();
            flate2::read::GzDecoder::new(&bytes[..])
                .read_to_end(&mut data)
                .unwrap();
            data
        })
        .unwrap();
        for (key, value_to_set) in [("version", json!(1)), ("index", json!({}))] {
            let old = value[key].clone();
            value[key] = value_to_set;
            let mut gzip =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gzip.write_all(&serde_json::to_vec(&value).unwrap())
                .unwrap();
            assert!(decode(&gzip.finish().unwrap(), chrono::Utc::now()).is_err());
            value[key] = old;
        }
        let mut old = index;
        old.built_at = (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
        assert!(decode(&encode(&old).unwrap().0, chrono::Utc::now()).is_err());
        conn.lock()
            .unwrap()
            .execute("DROP TABLE job_copy_index_snapshots", [])
            .unwrap();
        assert!(load(&audit).is_err());
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread")]
async fn interrupted_write_keeps_previous_generation_and_costs_one_request_per_part() {
    use rand::{Rng, SeedableRng};
    let (audit, conn, queries) = crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
    tokio::task::spawn_blocking(move || {
        let old = fixture(3); save(&audit,&old).unwrap();
        let mut next = fixture(4);
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        next.records[0].title = Some((0..2_000_000).map(|_| char::from(rng.gen_range(b'!'..=b'~'))).collect());
        let parts = encode(&next).unwrap().0.len().div_ceil(CHUNK_BYTES);
        assert!(parts > 1);
        conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_snapshot BEFORE INSERT ON job_copy_index_snapshots WHEN NEW.part=1 BEGIN SELECT RAISE(ABORT,'simulated'); END").unwrap();
        assert!(save(&audit,&next).is_err());
        assert_same(&load(&audit).unwrap().unwrap(),&old);
        conn.lock().unwrap().execute_batch("DROP TRIGGER fail_snapshot").unwrap();
        queries.lock().unwrap().clear();
        save(&audit,&next).unwrap();
        assert_eq!(queries.lock().unwrap().len(),parts);
        assert_same(&load(&audit).unwrap().unwrap(),&next);
        let generations:i64=conn.lock().unwrap().query_row("SELECT COUNT(DISTINCT generation) FROM job_copy_index_snapshots",[],|r|r.get(0)).unwrap();
        assert_eq!(generations,1);
    }).await.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
async fn worker_serves_restored_index_then_replaces_and_saves_it() {
    use axum::routing::any;
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let initial = Arc::new(fixture(1));
    persist(initial.clone(), Some(audit.clone())).await;
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let (seen, gate) = (entered.clone(), release.clone());
    let app = Router::new().fallback(any(move || {
        let (seen, gate) = (seen.clone(), gate.clone());
        async move {
            seen.notify_one();
            let _permit = gate.acquire().await.unwrap();
            Json(json!({"results":[]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = Arc::new(JobReadService::for_test(format!(
        "http://{}",
        listener.local_addr().unwrap()
    )));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let file = tempfile::NamedTempFile::new().unwrap();
    rusqlite::Connection::open(file.path()).unwrap().execute_batch("CREATE TABLE insight_title_pref(norm_title TEXT); INSERT INTO insight_title_pref VALUES('配送')").unwrap();
    let db = crate::db::local_sqlite::LocalDb::new(file.path().to_str().unwrap()).unwrap();
    start_worker(&service, db, Some(audit.clone()));
    tokio::time::timeout(Duration::from_secs(10), entered.notified())
        .await
        .unwrap();
    {
        let state = service.listing_index.state.read().await;
        assert!(state.refreshing);
        assert_same(state.snapshot.as_ref().unwrap(), &initial);
    }
    release.add_permits(20);
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let state = service.listing_index.state.read().await;
            if !state.refreshing && state.snapshot.as_ref().unwrap().records.is_empty() {
                break;
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let a = audit.clone();
            if tokio::task::spawn_blocking(move || load(&a).unwrap().unwrap().records.is_empty())
                .await
                .unwrap()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    server.abort();
    restore(&service.listing_index, None).await;
}
#[test]
fn measure_41000_body_free_rows() {
    let index = fixture(41_000);
    let (bytes, json_bytes) = encode(&index).unwrap();
    let parts = bytes.len().div_ceil(CHUNK_BYTES);
    println!("snapshot measurement: rows=41000 deals=500 json_bytes={json_bytes} gzip_bytes={} parts={parts} base64_bytes={}",bytes.len(),STANDARD.encode(&bytes).len());
    assert_same(&decode(&bytes, chrono::Utc::now()).unwrap(), &index);
}
