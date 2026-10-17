use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn round_trip_versions_corruption_and_independent_tables() {
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    tokio::task::spawn_blocking(move || {
        let built = Utc::now() - chrono::Duration::days(2);
        let data = json!({"count":1,"listings":[{"id":"123","title":"合成求人"}]});
        for kind in [Kind::Recent, Kind::Facts] {
            save(&audit, kind, &data, built).unwrap();
            let restored: Saved<Value> = load(&audit, kind).unwrap().unwrap();
            assert_eq!(restored.data, data);
            assert_eq!(restored.built_at, built);
            conn.lock()
                .unwrap()
                .execute(&format!("UPDATE {} SET version=999", kind.table()), [])
                .unwrap();
            assert!(load::<Value>(&audit, kind).unwrap().is_none());
            save(&audit, kind, &data, built).unwrap();
            conn.lock()
                .unwrap()
                .execute(&format!("UPDATE {} SET payload='broken'", kind.table()), [])
                .unwrap();
            assert!(load::<Value>(&audit, kind).is_err());
            save(&audit, kind, &data, built).unwrap();
            conn.lock()
                .unwrap()
                .execute(
                    &format!(
                        "UPDATE {} SET built_at='2000-01-01T00:00:00Z'",
                        kind.table()
                    ),
                    [],
                )
                .unwrap();
            assert!(load::<Value>(&audit, kind).is_err());
            // The SQL format number is not sufficient: validate the compressed envelope too.
            let mut bad = Saved {
                version: 999,
                built_at: built,
                data: &data,
            };
            let mut gzip =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gzip.write_all(&serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(decode::<Value>(&gzip.finish().unwrap(), Utc::now()).is_err());
            bad.version = VERSION;
            bad.built_at = Utc::now() + chrono::Duration::hours(1);
            assert!(decode::<Value>(&encode(&data, bad.built_at).unwrap().0, Utc::now()).is_err());
        }
        save(&audit, Kind::Recent, &data, built).unwrap();
        save(&audit, Kind::Facts, &json!({"different":true}), built).unwrap();
        assert_eq!(
            load::<Value>(&audit, Kind::Recent).unwrap().unwrap().data,
            data
        );
        assert!(
            instant(built, Duration::from_secs(3600)).elapsed() >= Duration::from_secs(2 * 86400)
        );
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn incomplete_generation_keeps_previous_and_success_removes_old_generations() {
    use rand::{Rng, SeedableRng};
    let (audit, conn, queries) = crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
    tokio::task::spawn_blocking(move || {
        let built = Utc::now();
        let old = json!({"value":"合成の前回結果"});
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let large: String = (0..2_000_000).map(|_| char::from(rng.gen_range(b'!'..=b'~'))).collect();
        for kind in [Kind::Recent,Kind::Facts] {
            save(&audit,kind,&old,built).unwrap();
            conn.lock().unwrap().execute_batch(&format!("CREATE TRIGGER fail_report BEFORE INSERT ON {} WHEN NEW.part=1 BEGIN SELECT RAISE(ABORT,'simulated'); END",kind.table())).unwrap();
            assert!(save(&audit,kind,&large,built).is_err());
            assert_eq!(load::<Value>(&audit,kind).unwrap().unwrap().data,old);
            conn.lock().unwrap().execute_batch("DROP TRIGGER fail_report").unwrap();
            queries.lock().unwrap().clear();
            save(&audit,kind,&large,built).unwrap();
            let parts = encode(&large,built).unwrap().0.len().div_ceil(CHUNK_BYTES);
            assert!(parts > 1);
            assert_eq!(queries.lock().unwrap().len(),parts);
            assert_eq!(load::<String>(&audit,kind).unwrap().unwrap().data,large);
            let generations:i64 = conn.lock().unwrap().query_row(&format!("SELECT COUNT(DISTINCT generation) FROM {}",kind.table()),[],|r|r.get(0)).unwrap();
            assert_eq!(generations,1);
        }
    }).await.unwrap();
}

#[tokio::test]
async fn missing_or_failed_audit_is_optional() {
    assert!(restore::<Value>(None, Kind::Recent).await.is_none());
    persist(None, Kind::Facts, Arc::new(json!({"live":1})), Utc::now()).await;
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    conn.lock()
        .unwrap()
        .execute("DROP TABLE job_copy_recent_snapshots", [])
        .unwrap();
    assert!(restore::<Value>(Some(audit.clone()), Kind::Recent)
        .await
        .is_none());
    persist(
        Some(audit),
        Kind::Recent,
        Arc::new(json!({"count":0})),
        Utc::now(),
    )
    .await;
}
