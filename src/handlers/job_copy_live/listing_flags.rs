//! 顧客（法人）の画面の求人の表に、ホームと同じ分類（応募がゼロになった・課金が高いまま応募が少ない など）を付ける。
//! 分類の基準・判定はホームと同じ `listings::enrich_recent_changes`（中身は listing_home.rs の classify）を使い回す。
//! 新しい基準は作らない。完成済みの索引だけを読み、HubSpot は呼ばない。
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

/// 分類の結果を、求人 ID → 分類のキーと、カードの見出し・基準の文に整える。
fn shape(data: &Value) -> Value {
    if data["report"]["breakdown"]["status"] != "ready" {
        return json!({"status":"preparing"});
    }
    let flags: serde_json::Map<String, Value> = data["report"]["listings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some((
                row["id"].as_str()?.to_owned(),
                row["classifications"].clone(),
            ))
        })
        .collect();
    let cards: Vec<Value> = data["report"]["breakdown"]["cards"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|card| json!({"key":card["key"],"title":card["title"],"rule":card["rule"]}))
        .collect();
    json!({"status":"ready","flags":flags,"cards":cards})
}

async fn classify_ids(
    service: &JobReadService,
    ids: &[String],
    today: chrono::NaiveDate,
    selection: Option<(&owner_todo::Snapshot, &owner_todo::OwnerQuery, &str)>,
) -> Value {
    let mut data =
        json!({"report":{"listings": ids.iter().map(|id| json!({"id":id})).collect::<Vec<_>>()}});
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

    fn record(id: &str, status: &str, perf: Option<Value>) -> Record {
        let mut properties = json!({"hs_name":format!("合成求人{id}"),"id_hrhakkaa":format!("HR-{id}"),"kyuujin_status":status});
        if let Some(perf) = perf {
            properties["indeed_weekly_perf"] = json!(perf.to_string());
        }
        serde_json::from_value(json!({"id":id,"properties":properties})).unwrap()
    }

    /// 最新の週を今日の週にした 8 週。prev は前の 4 週、recent は直近 4 週の (課金, 応募)。
    fn perf(id: &str, prev: [(i64, i64); 4], recent: [(i64, i64); 4]) -> Value {
        let today = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive();
        let latest = today - chrono::Duration::days(6);
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
    async fn same_classification_as_home_for_each_listing() {
        let service = JobReadService::for_test("http://127.0.0.1:9".into());
        let records = vec![
            // 前の4週 3件 → 直近4週 0件、課金 2万円: 応募がゼロになった + 課金が高いまま応募が少ない。
            record(
                "1",
                "公開中",
                Some(perf(
                    "1",
                    [(0, 1), (0, 1), (0, 1), (0, 0)],
                    [(5000, 0), (5000, 0), (5000, 0), (5000, 0)],
                )),
            ),
            // 前 4件 → 直近 8件、課金 4,000円 (1件あたり 500円): 応募が増えた。
            record(
                "2",
                "公開中",
                Some(perf(
                    "2",
                    [(0, 1), (0, 1), (0, 1), (0, 1)],
                    [(1000, 2), (1000, 2), (1000, 2), (1000, 2)],
                )),
            ),
            // 前 4件 → 直近 4件: ほぼ同じ。
            record(
                "3",
                "公開中",
                Some(perf(
                    "3",
                    [(0, 1), (0, 1), (0, 1), (0, 1)],
                    [(0, 1), (0, 1), (0, 1), (0, 1)],
                )),
            ),
            record("4", "公開終了", Some(perf("4", [(0, 1); 4], [(0, 0); 4]))),
            record("5", "公開中", None),
        ];
        listings::seed_recent_change_index(&service, &records).await;
        let today = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive();
        let ids: Vec<String> = ["1", "2", "3", "4", "5", "99"].map(String::from).to_vec();
        let data = classify_ids(&service, &ids, today, None).await;
        assert_eq!(data["status"], "ready");
        assert_eq!(data["flags"]["1"], json!(["to_zero", "costly"]));
        assert_eq!(data["flags"]["2"], json!(["more"]));
        assert_eq!(data["flags"]["3"], json!(["flat"]));
        assert_eq!(data["flags"]["4"], json!(["out_of_scope"]));
        assert_eq!(data["flags"]["5"], json!(["unknown"]));
        assert_eq!(data["flags"]["99"], json!(["unknown"]));
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
    async fn preparing_index_is_not_reported_as_no_flags() {
        let service = JobReadService::for_test("http://127.0.0.1:9".into());
        let today = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive();
        let data = classify_ids(&service, &["1".to_owned()], today, None).await;
        assert_eq!(data, json!({"status":"preparing"}));
    }
}
