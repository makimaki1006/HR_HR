//! 顧客（法人）の画面の求人の表に、ホームと同じ分類（応募がゼロになった・課金が高いまま応募が少ない など）を付ける。
//! 分類の基準・判定はホームと同じ `listings::enrich_recent_changes`（中身は listing_home.rs の classify）を使い回す。
//! 新しい基準は作らない。完成済みの索引だけを読み、HubSpot は呼ばない。
//! 判定に使った数字（前4週・直近4週の応募、直近4週の課金、最新の週の終わり）も返し、画面の表の数字との違いを見せられるようにする。
use super::*;

/// 1 回に分類する求人の上限。画面は 200 件ずつに分けて読む。
const MAX_IDS: usize = 200;

#[derive(Debug, Default, Deserialize)]
pub(super) struct FlagQuery {
    owner_field: Option<String>,
    owner: Option<String>,
    /// 求人 ID のカンマ区切り。
    ids: Option<String>,
}

fn parse_ids(raw: Option<&str>) -> Result<Vec<String>, ReadError> {
    let ids: Vec<String> = raw
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() || ids.len() > MAX_IDS {
        return Err(ReadError(StatusCode::BAD_REQUEST, "listing_flags_invalid"));
    }
    for id in &ids {
        valid_id(id)?;
    }
    Ok(ids)
}

/// 分類の結果を、求人 ID → 分類のキー、求人 ID → 判定に使った数字、カードの見出し・基準の文に整える。
fn shape(data: &Value) -> Value {
    if data["report"]["breakdown"]["status"] != "ready" {
        return json!({"status":"preparing"});
    }
    let rows = || {
        data["report"]["listings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| Some((row["id"].as_str()?.to_owned(), row)))
    };
    let flags: serde_json::Map<String, Value> = rows()
        .map(|(id, row)| (id, row["classifications"].clone()))
        .collect();
    let numbers: serde_json::Map<String, Value> =
        rows().map(|(id, row)| (id, row["perf"].clone())).collect();
    let cards: Vec<Value> = data["report"]["breakdown"]["cards"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|card| json!({"key":card["key"],"title":card["title"],"rule":card["rule"]}))
        .collect();
    json!({"status":"ready","flags":flags,"numbers":numbers,"cards":cards})
}

pub(super) async fn classify_ids(
    service: &JobReadService,
    ids: &[String],
    today: chrono::NaiveDate,
    selection: Option<(&owner_todo::Snapshot, &owner_todo::OwnerQuery, &str)>,
) -> Value {
    let mut data = json!({"report":{"listings": ids.iter().map(|id| json!({"id":id,"with_perf":true})).collect::<Vec<_>>()}});
    listings::enrich_recent_changes(service, &mut data, today, selection).await;
    shape(&data)
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<FlagQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    let owner = owner_todo::OwnerQuery {
        owner_field: query.owner_field.clone(),
        owner: query.owner.clone(),
    };
    owner.validate()?;
    let ids = parse_ids(query.ids.as_deref())?;
    let service = listings::service(&access)?;
    let today = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive();
    // 担当者の範囲（中央値を取る求人の範囲）は、ホーム・直近7日の変更とそろえる。
    let data = if owner.owner.is_some() || owner.owner_field.is_some() {
        let view = owner_todo::view(service).await;
        match view.snapshot {
            Some(snapshot) => {
                classify_ids(service, &ids, today, Some((&snapshot, &owner, &email))).await
            }
            None => json!({"status":if view.failed {"failed"} else {"preparing"}}),
        }
    } else {
        classify_ids(service, &ids, today, None).await
    };
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(data)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> chrono::NaiveDate {
        (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive()
    }

    fn record(id: &str, status: &str, perf: Option<Value>) -> Record {
        let mut properties = json!({"hs_name":format!("合成求人{id}"),"id_hrhakkaa":format!("HR-{id}"),"kyuujin_status":status});
        if let Some(perf) = perf {
            properties["indeed_weekly_perf"] = json!(perf.to_string());
        }
        serde_json::from_value(json!({"id":id,"properties":properties})).unwrap()
    }

    /// 8 週の週ごとの実績。最新の週は今日から weeks_ago 週前に終わる。prev は前の 4 週、recent は直近 4 週の (課金, 応募)。
    fn perf_ending(
        id: &str,
        weeks_ago: i64,
        prev: [(i64, i64); 4],
        recent: [(i64, i64); 4],
    ) -> Value {
        let latest = today() - chrono::Duration::days(6 + 7 * weeks_ago);
        let rows: Vec<Value> = prev
            .iter()
            .chain(recent.iter())
            .enumerate()
            .map(|(i, (cost, apps))| {
                let start = latest - chrono::Duration::days(7 * (7 - i as i64));
                json!({"period_start":start.to_string(),"period_end":(start + chrono::Duration::days(6)).to_string(),"cost_yen":cost,"applications":apps})
            })
            .collect();
        json!({"schema_version":1,"source":"hrhacker","job_id":format!("HR-{id}"),"rows":rows})
    }
    fn perf(id: &str, prev: [(i64, i64); 4], recent: [(i64, i64); 4]) -> Value {
        perf_ending(id, 0, prev, recent)
    }
    async fn classify(records: Vec<Record>, ids: &[&str]) -> Value {
        let service = JobReadService::for_test("http://127.0.0.1:9".into());
        listings::seed_recent_change_index(&service, &records).await;
        let ids: Vec<String> = ids.iter().map(|id| (*id).to_owned()).collect();
        classify_ids(&service, &ids, today(), None).await
    }

    #[test]
    fn ids_are_deduplicated_and_must_be_numeric_and_at_most_200() {
        assert_eq!(parse_ids(Some("3, 1,3")).unwrap(), vec!["1", "3"]);
        assert!(parse_ids(Some("1,abc")).is_err());
        assert!(parse_ids(Some("")).is_err());
        assert!(parse_ids(None).is_err());
        let many = (1..=201)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse_ids(Some(&many)).is_err());
        let ok = (1..=200)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(parse_ids(Some(&ok)).unwrap().len(), 200);
    }

    #[tokio::test]
    async fn same_classification_as_home_for_each_listing_with_the_numbers_used() {
        let data = classify(
            vec![
                // 前の4週 3件 → 直近4週 0件、課金 2万円: 応募がゼロになった + 課金が高いまま応募が少ない。
                record(
                    "1",
                    "公開中",
                    Some(perf("1", [(0, 1), (0, 1), (0, 1), (0, 0)], [(5000, 0); 4])),
                ),
                // 前 4件 → 直近 8件、課金 4,000円: 応募が増えた。
                record("2", "公開中", Some(perf("2", [(0, 1); 4], [(1000, 2); 4]))),
                // 前 4件 → 直近 4件: ほぼ同じ。
                record("3", "公開中", Some(perf("3", [(0, 1); 4], [(0, 1); 4]))),
                record("4", "公開終了", Some(perf("4", [(0, 1); 4], [(0, 0); 4]))),
                record("5", "公開中", None),
            ],
            &["1", "2", "3", "4", "5", "99"],
        )
        .await;
        assert_eq!(data["status"], "ready");
        assert_eq!(data["flags"]["1"], json!(["to_zero", "costly"]));
        assert_eq!(data["flags"]["2"], json!(["more"]));
        assert_eq!(data["flags"]["3"], json!(["flat"]));
        assert_eq!(data["flags"]["4"], json!(["out_of_scope"]));
        assert_eq!(data["flags"]["5"], json!(["unknown"]));
        assert_eq!(data["flags"]["99"], json!(["unknown"]));
        assert_eq!(
            data["numbers"]["1"],
            json!({"prev_apps":3,"recent_apps":0,"recent_cost":20000,"latest_end":today().to_string()})
        );
        assert_eq!(data["numbers"]["2"]["recent_apps"], 8);
        assert_eq!(data["numbers"]["5"], Value::Null);
        assert_eq!(data["numbers"]["99"], Value::Null);
        let keys: Vec<&str> = data["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|card| card["key"].as_str().unwrap())
            .collect();
        assert_eq!(
            keys,
            vec![
                "zero_stay",
                "to_zero",
                "less",
                "more",
                "flat",
                "costly",
                "stale",
                "out_of_scope",
                "unknown"
            ]
        );
        assert_eq!(data["cards"][1]["title"], "応募がゼロになった求人");
    }

    #[tokio::test]
    async fn costly_when_cost_per_application_is_at_least_twice_the_median() {
        // 応募1件あたり: 1 = 500円, 2 = 3,000円, 3 = 1,000円, 4 = 1,500円。中央値 (1,000 + 1,500) / 2 = 1,250円、その2倍 = 2,500円。
        let data = classify(
            vec![
                record("1", "公開中", Some(perf("1", [(0, 2); 4], [(1000, 2); 4]))),
                record("2", "公開中", Some(perf("2", [(0, 1); 4], [(3000, 1); 4]))),
                record(
                    "3",
                    "公開中",
                    Some(perf(
                        "3",
                        [(0, 3); 4],
                        [(2500, 3), (2500, 3), (2500, 2), (2500, 2)],
                    )),
                ),
                record(
                    "4",
                    "公開中",
                    Some(perf(
                        "4",
                        [(0, 3); 4],
                        [(3750, 3), (3750, 3), (3750, 2), (3750, 2)],
                    )),
                ),
            ],
            &["1", "2", "3", "4"],
        )
        .await;
        let costly = |id: &str| {
            data["flags"][id]
                .as_array()
                .unwrap()
                .contains(&json!("costly"))
        };
        // 2: 課金 12,000円・応募 4件 = 3,000円 ≥ 2,500円 → 付く。
        assert!(costly("2"));
        // 4: 課金 15,000円・応募 10件 = 1,500円 < 2,500円 → 課金が1万円以上でも付かない。
        assert!(!costly("4"));
        // 1: 課金 4,000円は下限の1万円未満。3: 10,000円・10件 = 1,000円。
        assert!(!costly("1"));
        assert!(!costly("3"));
        assert_eq!(data["numbers"]["2"]["recent_cost"], 12000);
    }

    #[tokio::test]
    async fn zero_stay_less_and_stale_follow_the_home_rules() {
        let data = classify(
            vec![
                record("1", "公開中", Some(perf("1", [(0, 0); 4], [(0, 0); 4]))),
                // 前 10件 → 直近 5件: 2件以上少ない。
                record(
                    "2",
                    "公開中",
                    Some(perf(
                        "2",
                        [(0, 3), (0, 3), (0, 2), (0, 2)],
                        [(0, 2), (0, 1), (0, 1), (0, 1)],
                    )),
                ),
                // 最新の週が 3 週前（21日前）に終わっている: データが止まっている（14日より前）。
                record(
                    "3",
                    "公開中",
                    Some(perf_ending("3", 3, [(0, 1); 4], [(0, 1); 4])),
                ),
                // 2 週前（14日前）に終わっている: まだ止まっていない。
                record(
                    "4",
                    "公開中",
                    Some(perf_ending("4", 2, [(0, 1); 4], [(0, 1); 4])),
                ),
            ],
            &["1", "2", "3", "4"],
        )
        .await;
        assert_eq!(data["flags"]["1"], json!(["zero_stay"]));
        assert_eq!(data["flags"]["2"], json!(["less"]));
        assert_eq!(data["flags"]["3"], json!(["flat", "stale"]));
        assert_eq!(data["flags"]["4"], json!(["flat"]));
        assert_eq!(
            data["numbers"]["3"]["latest_end"],
            (today() - chrono::Duration::days(21)).to_string()
        );
    }

    #[tokio::test]
    async fn preparing_index_is_not_reported_as_no_flags() {
        let service = JobReadService::for_test("http://127.0.0.1:9".into());
        let data = classify_ids(&service, &["1".to_owned()], today(), None).await;
        assert_eq!(data, json!({"status":"preparing"}));
    }

    #[tokio::test]
    async fn rejects_requests_without_login_before_reading_the_index() {
        use axum::{body::to_bytes, body::Body, http::Request};
        use tower::ServiceExt;
        let service = Arc::new(JobReadService::for_test("http://127.0.0.1:9".into()));
        listings::seed_recent_change_index(
            &service,
            &[record(
                "1",
                "公開中",
                Some(perf("1", [(0, 1); 4], [(0, 0); 4])),
            )],
        )
        .await;
        let access = Access {
            service: Some(service),
            moc_path: None,
            moc_drive: Ok(None),
            snapshot_reader: None,
            snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
            resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            images: None,
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
        };
        let app = Router::new()
            .route("/api/job-copy/listing-flags", get(read))
            .layer(Extension(access))
            .with_state(super::super::integration_tests::moc_state())
            .layer(
                tower_sessions::SessionManagerLayer::new(tower_sessions::MemoryStore::default())
                    .with_secure(false),
            );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/job-copy/listing-flags?ids=1&owner=all")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(body, json!({"code":"login_required"}));
    }
}
