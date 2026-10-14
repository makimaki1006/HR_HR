use super::*;

fn deal(id: &str, pipeline: &str, stage: &str, created: &str) -> Record {
    serde_json::from_value(json!({"id":id,"properties":{
        "dealname":format!("合成案件{id}"),"pipeline":pipeline,"dealstage":stage,"createdate":created,
        "contract_start_date":"2026-04-01","contract_expiration_date":"2027-03-31"}}))
    .unwrap()
}
fn defs() -> Defs {
    parse_defs(&json!({"results":[
        {"id":"21596025","label":"リクロジ_納品管理","stages":[
            {"id":"s1","label":"納品中","metadata":{"isClosed":"false"}},
            {"id":"s9","label":"契約終了","metadata":{"isClosed":"true"}}]},
        {"id":"22417753","label":"商談管理_計上","stages":[{"id":"x","label":"計上済","metadata":{"isClosed":"true"}}]}
    ]}))
}
fn listing(id: &str, deals: &[&str], status: &str, apps: Option<(i64, i64)>) -> Listing {
    Listing {
        id: id.into(),
        media: "hrh",
        media_job_id: format!("HR-{id}"),
        account_id: None,
        title: None,
        prefecture: None,
        municipality: None,
        category: None,
        indeed_title: None,
        indeed_title_state: "unset",
        publication_status: Some(status.into()),
        draft_status: None,
        last_csv_detected_at: None,
        deal_ids: deals.iter().map(|d| d.to_string()).collect(),
        summary: Summary::default(),
        indexed_application_count: None,
        perf: apps.map(|(a, c)| home::PerfDigest {
            latest_end: chrono::NaiveDate::from_ymd_opt(2026, 10, 4).unwrap(),
            recent_apps: a,
            recent_cost: c,
            prev_apps: 0,
        }),
    }
}

#[test]
fn only_the_two_delivery_pipelines_are_allowed() {
    assert!(is_delivery(Some("21596025")) && is_delivery(Some("743961896")));
    for other in [
        "22417753",
        "743705507",
        "741140824",
        "21724969",
        "default",
        "",
    ] {
        assert!(!is_delivery(Some(other)), "{other}");
    }
    assert!(!is_delivery(None));
}

#[test]
fn deals_in_other_pipelines_are_dropped_and_names_come_from_definitions() {
    let records = vec![
        deal("1", "21596025", "s1", "2026-09-01T00:00:00Z"),
        deal("2", "22417753", "x", "2026-09-02T00:00:00Z"),
        deal("3", "743961896", "zz", "2026-09-03T00:00:00Z"),
    ];
    let rows = build_rows(&records, &defs(), None);
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["3", "1"]
    );
    let one = rows.iter().find(|r| r.id == "1").unwrap();
    assert_eq!(one.pipeline_label, "リクロジ_納品管理");
    assert_eq!(one.stage_label.as_deref(), Some("納品中"));
    assert_eq!(one.created_at.as_deref(), Some("2026-09-01"));
    assert_eq!(one.contract_end.as_deref(), Some("2027-03-31"));
    // 定義に無いパイプライン・ステージでも、呼び名で出し、ステージは未取得にする。
    let three = rows.iter().find(|r| r.id == "3").unwrap();
    assert_eq!(three.pipeline_label, "請負契約_納品管理");
    assert_eq!(three.stage_label, None);
    assert!(three.active);
    assert_eq!((one.listings_total, one.recent_apps), (None, None));
}

#[test]
fn active_deals_come_first_then_newest_created() {
    let records = vec![
        deal("1", "21596025", "s9", "2026-10-01T00:00:00Z"),
        deal("2", "21596025", "s1", "2026-05-01T00:00:00Z"),
        deal("3", "21596025", "s1", "2026-08-01T00:00:00Z"),
    ];
    let rows = build_rows(&records, &defs(), None);
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["3", "2", "1"]
    );
    assert!(!rows[2].active);
}

#[test]
fn listing_counts_use_live_listings_and_their_four_weeks() {
    let records = vec![
        deal("1", "21596025", "s1", "2026-09-01T00:00:00Z"),
        deal("2", "21596025", "s1", "2026-08-01T00:00:00Z"),
    ];
    let listings = vec![
        listing("a", &["1"], "掲載中", Some((3, 12_000))),
        listing("b", &["1", "2"], "掲載中", Some((2, 8_000))),
        listing("c", &["1"], "掲載終了", Some((9, 99_000))),
        listing("d", &["2"], "掲載中", None),
    ];
    let rows = build_rows(&records, &defs(), Some(&listings));
    let one = rows.iter().find(|r| r.id == "1").unwrap();
    assert_eq!((one.listings_total, one.listings_live), (Some(3), Some(2)));
    assert_eq!((one.recent_apps, one.recent_cost), (Some(5), Some(20_000)));
    let two = rows.iter().find(|r| r.id == "2").unwrap();
    assert_eq!((two.listings_total, two.listings_live), (Some(2), Some(2)));
    // 週ごとの実績のある掲載中の求人が b だけ。
    assert_eq!((two.recent_apps, two.recent_cost), (Some(2), Some(8_000)));
}
