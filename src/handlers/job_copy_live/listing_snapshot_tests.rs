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
            "listing_started_on":"2026-09-01", "listing_ended_on":"2026-10-15", "listing_planned_end_on":"2026-10-31",
            "deal_ids":[(n%500+800000).to_string()], "summary":{
                "occupation":"配送", "location":"東京都新宿区", "salary_kind":"月給",
                "salary_min":(200000+n%10000).to_string(), "salary_max":"350000", "hours":"9:00〜18:00", "holidays":"週休2日", "body_changed_at":"2026-10-15T01:00:00Z"
            }
        })).unwrap();
        row.indexed_application_count = Some(n % 31);
        row.body_summary = Some(row.summary.clone());
        row.perf = Some(home::PerfDigest {
            comparison_recent: None,
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
                assignment: owner_todo::Assignment {
                    acquired: true,
                    consultant: Some("11".into()),
                    hubspot_owner_id: Some("22".into()),
                    actionyotei: Some("2026-10-16".into()),
                    actionnaiyou: Some("call".into()),
                    contract_expiration_date: Some("2026-11-15".into()),
                },
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
        assert_eq!(a.assignment, b.assignment);
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
        save(&audit, &index, None).unwrap();
        assert_same(&load(&audit).unwrap().unwrap().0, &index);
        conn.lock()
            .unwrap()
            .execute("UPDATE job_copy_index_snapshots SET version=3", [])
            .unwrap();
        assert!(load(&audit).unwrap().is_none());
        save(&audit, &index, None).unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE job_copy_index_snapshots SET built_at='2000-01-01T00:00:00Z'",
                [],
            )
            .unwrap();
        assert!(load(&audit).unwrap().is_none());
        save(&audit, &index, None).unwrap();
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
        for (key, value_to_set) in [
            ("version", json!(1)),
            ("version", json!(2)),
            ("version", json!(3)),
            ("index", json!({})),
        ] {
            let old = value[key].clone();
            value[key] = value_to_set;
            let mut gzip =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gzip.write_all(&serde_json::to_vec(&value).unwrap())
                .unwrap();
            assert!(decode(&gzip.finish().unwrap(), chrono::Utc::now(), None).is_err());
            value[key] = old;
        }
        let mut old = index;
        old.built_at = (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
        assert!(decode(&encode(&old).unwrap().0, chrono::Utc::now(), None).is_err());
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
        let old = fixture(3); save(&audit, &old, None).unwrap();
        let mut next = fixture(4);
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        next.records[0].title = Some((0..2_000_000).map(|_| char::from(rng.gen_range(b'!'..=b'~'))).collect());
        let parts = encode(&next).unwrap().0.len().div_ceil(CHUNK_BYTES);
        assert!(parts > 1);
        conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_snapshot BEFORE INSERT ON job_copy_index_snapshots WHEN NEW.part=1 BEGIN SELECT RAISE(ABORT,'simulated'); END").unwrap();
        assert!(save(&audit, &next, None).is_err());
        assert_same(&load(&audit).unwrap().unwrap().0,&old);
        conn.lock().unwrap().execute_batch("DROP TRIGGER fail_snapshot").unwrap();
        queries.lock().unwrap().clear();
        save(&audit, &next, None).unwrap();
        assert_eq!(queries.lock().unwrap().len(),parts);
        assert_same(&load(&audit).unwrap().unwrap().0,&next);
        let generations:i64=conn.lock().unwrap().query_row("SELECT COUNT(DISTINCT generation) FROM job_copy_index_snapshots",[],|r|r.get(0)).unwrap();
        assert_eq!(generations,1);
    }).await.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
async fn worker_serves_restored_index_then_replaces_and_saves_it() {
    use axum::routing::any;
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    // 6 時間より古い控え: 起動時に作り直す (新しい控えは下の別のテスト)
    let mut old = fixture(1);
    old.built_at = (chrono::Utc::now() - chrono::Duration::hours(7)).to_rfc3339();
    let initial = Arc::new(old);
    persist(initial.clone(), Some(audit.clone()), &mut None).await;
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
            // The worker may be mid-save: the last INSERT's trigger can drop the generation
            // this read picked, so a failed read here means "not yet", not a broken snapshot.
            if tokio::task::spawn_blocking(
                move || matches!(load(&a), Ok(Some((index, _))) if index.records.is_empty()),
            )
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
#[tokio::test(flavor = "multi_thread")]
async fn unchanged_rebuild_moves_the_time_without_resending_the_copy() {
    let (audit, conn, queries) = crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
    tokio::task::spawn_blocking(move || {
        // 30 時間前に作った中身を、そのあと同じ中身のまま作り直し続けた場合。
        let mut first = fixture(12);
        first.built_at = (chrono::Utc::now() - chrono::Duration::hours(30)).to_rfc3339();
        let saved = save(&audit, &first, None).unwrap();
        let payload_before: String = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT group_concat(payload,'') FROM job_copy_index_snapshots",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let mut rebuilt = fixture(12);
        rebuilt.built_at = chrono::Utc::now().to_rfc3339();
        queries.lock().unwrap().clear();
        let kept = save(&audit, &rebuilt, Some(&saved)).unwrap();
        assert_eq!(kept, saved);
        // 送るのは時刻を進める UPDATE 1 文だけ。中身は送り直さない。
        let sent = queries.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].contains("UPDATE job_copy_index_snapshots"));
        let payload_after: String = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT group_concat(payload,'') FROM job_copy_index_snapshots",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(payload_before, payload_after);

        // 中身は 30 時間前のものでも、確かめた時刻が新しいので復元でき、時刻は確かめた時刻になる。
        let (restored, restored_saved) = load(&audit).unwrap().unwrap();
        assert_eq!(restored.built_at, rebuilt.built_at);
        assert_same(&restored, &rebuilt);
        // 復元した控えの要約値は保存したときと同じ (再起動後の作り直しも保存し直さない)。
        assert_eq!(restored_saved, saved);

        // 中身が 1 か所でも変われば全部を保存し、新しい控えになる。
        let mut changed = fixture(12);
        changed.records[3].title = Some("変わった求人".into());
        queries.lock().unwrap().clear();
        let next = save(&audit, &changed, Some(&saved)).unwrap();
        assert_ne!(next.generation, saved.generation);
        assert!(queries.lock().unwrap().iter().all(|q| q.contains("INSERT")));
        assert_same(&load(&audit).unwrap().unwrap().0, &changed);

        // 前回の控えが消えていたら (UPDATE が 0 行)、全部を保存し直す。
        conn.lock()
            .unwrap()
            .execute("DELETE FROM job_copy_index_snapshots", [])
            .unwrap();
        let again = save(&audit, &changed, Some(&next)).unwrap();
        assert_ne!(again.generation, next.generation);
        assert_same(&load(&audit).unwrap().unwrap().0, &changed);
    })
    .await
    .unwrap();
}
#[test]
fn digest_ignores_only_the_build_time() {
    let mut a = fixture(3);
    let mut b = fixture(3);
    a.built_at = "2026-11-01T00:00:00+00:00".into();
    b.built_at = "2026-11-16T09:00:00+00:00".into();
    let d = |i: &Index| digest(&to_json(i).unwrap(), &i.built_at);
    assert_eq!(d(&a), d(&b));
    b.deals[0].count += 1;
    assert_ne!(d(&a), d(&b));
}
#[test]
fn measure_41000_body_free_rows() {
    let index = fixture(41_000);
    let (bytes, json_bytes) = encode(&index).unwrap();
    let parts = bytes.len().div_ceil(CHUNK_BYTES);
    println!("snapshot measurement: rows=41000 deals=500 json_bytes={json_bytes} gzip_bytes={} parts={parts} base64_bytes={}",bytes.len(),STANDARD.encode(&bytes).len());
    assert_same(&decode(&bytes, chrono::Utc::now(), None).unwrap().0, &index);
}

#[test]
fn reclogi_snapshot_round_trip_and_v3_rejection_without_network() {
    let mut index = fixture(2);
    for (n, row) in index.records.iter_mut().enumerate() {
        row.media = "reclogi";
        row.media_job_id = if n == 0 {
            "424242".into()
        } else {
            String::new()
        };
        row.account_id = None;
    }
    let (bytes, _) = encode(&index).unwrap();
    assert_same(&decode(&bytes, chrono::Utc::now(), None).unwrap().0, &index);
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_end(&mut raw)
        .unwrap();
    let mut saved: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(saved["version"], 4);
    saved["version"] = json!(3);
    let mut old = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    old.write_all(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(decode(&old.finish().unwrap(), chrono::Utc::now(), None).is_err());
}

#[test]
fn restored_copy_within_six_hours_waits_until_six_hours_after_it() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-11-16T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let at = |h: i64, m: i64| {
        (now - chrono::Duration::hours(h) - chrono::Duration::minutes(m)).to_rfc3339()
    };
    // 2 時間前の控え → 次の作り直しは 4 時間後 (控えの時刻 + 6 時間)
    assert_eq!(
        restored_wait(&at(2, 0), now),
        Some(Duration::from_secs(4 * 3600))
    );
    assert_eq!(
        restored_wait(&at(5, 59), now),
        Some(Duration::from_secs(60))
    );
    // ちょうど 6 時間・それより古い控えは起動時に作り直す
    assert_eq!(restored_wait(&at(6, 0), now), None);
    assert_eq!(restored_wait(&at(7, 0), now), None);
    // 未来の時刻・読めない時刻も作り直す (未来の控えは restore も採らない)
    assert_eq!(
        restored_wait(&(now + chrono::Duration::minutes(5)).to_rfc3339(), now),
        None
    );
    assert_eq!(restored_wait("not a time", now), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn worker_skips_the_startup_build_when_the_saved_copy_is_recent() {
    use axum::routing::any;
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    // 1 時間前に作った控え
    let mut recent = fixture(1);
    recent.built_at = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let initial = Arc::new(recent);
    persist(initial.clone(), Some(audit.clone()), &mut None).await;
    let paths = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = paths.clone();
    let app = Router::new().fallback(any(move |uri: axum::http::Uri| {
        let seen = seen.clone();
        async move {
            seen.lock().unwrap().push(uri.path().to_owned());
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
    // 復元されて「最新化中」が外れるまで待つ
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let state = service.listing_index.state.read().await;
            if state.snapshot.is_some() && !state.refreshing {
                break;
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    // 作り直しが走らない (求人の Search を呼ばない) ことを少し待って確かめる
    tokio::time::sleep(Duration::from_millis(500)).await;
    {
        let state = service.listing_index.state.read().await;
        assert!(!state.refreshing);
        // 画面の「○時点」は控えの時刻のまま
        assert_eq!(state.snapshot.as_ref().unwrap().built_at, initial.built_at);
        assert_same(state.snapshot.as_ref().unwrap(), &initial);
    }
    let searched = paths
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.contains("/search"))
        .count();
    assert_eq!(
        searched,
        0,
        "控えが新しいので求人を読み直さない: {:?}",
        paths.lock().unwrap()
    );
    server.abort();
}
