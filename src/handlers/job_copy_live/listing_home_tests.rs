use super::*;

pub(super) fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn perf(rows: &[(&str, &str, i64, i64)]) -> Value {
    json!({"rows": rows.iter().map(|(s, e, cost, apps)| json!({"period_start":s,"period_end":e,"cost_yen":cost,"applications":apps})).collect::<Vec<_>>()})
}
fn weeks(recent: [(i64, i64); 4], prev: [(i64, i64); 4]) -> Value {
    // 最新の週は 09-28（月）。前の 4 週 = 最新から 28〜55 日前、直近 4 週 = 0〜27 日前。
    let starts = [
        "2026-08-10",
        "2026-08-17",
        "2026-08-24",
        "2026-08-31",
        "2026-09-07",
        "2026-09-14",
        "2026-09-21",
        "2026-09-28",
    ];
    let mut rows = vec![];
    for (i, (cost, apps)) in prev.iter().enumerate() {
        rows.push((starts[i], starts[i], *cost, *apps));
    }
    for (i, (cost, apps)) in recent.iter().enumerate() {
        rows.push((starts[4 + i], starts[4 + i], *cost, *apps));
    }
    let rows: Vec<_> = rows
        .into_iter()
        .map(|(s, _, c, a)| {
            (
                s.to_owned(),
                (date(s) + chrono::Duration::days(6)).to_string(),
                c,
                a,
            )
        })
        .collect();
    json!({"rows": rows.iter().map(|(s, e, c, a)| json!({"period_start":s,"period_end":e,"cost_yen":c,"applications":a})).collect::<Vec<_>>()})
}
pub(super) fn listing(
    id: &str,
    deals: &[&str],
    status: &str,
    state: &'static str,
    perf: Option<&Value>,
) -> Listing {
    Listing {
        id: id.into(),
        media: "hrh",
        media_job_id: format!("HR-{id}"),
        account_id: None,
        title: Some(format!("合成求人{id}")),
        prefecture: None,
        municipality: None,
        category: None,
        indeed_title: None,
        indeed_title_state: state,
        publication_status: Some(status.into()),
        draft_status: None,
        last_csv_detected_at: None,
        deal_ids: deals.iter().map(|d| d.to_string()).collect(),
        summary: Summary::default(),
        body_summary: None,
        indexed_application_count: None,
        perf: perf.and_then(PerfDigest::from_json),
    }
}
fn map() -> CompanyMap {
    CompanyMap {
        built_for: String::new(),
        at: None,
        deal_company: [("d1", "c1"), ("d2", "c1"), ("d3", "c2")]
            .into_iter()
            .map(|(d, c)| (d.to_owned(), c.to_owned()))
            .collect(),
        names: [("c1", "合成運輸"), ("c2", "合成介護")]
            .into_iter()
            .map(|(c, n)| (c.to_owned(), Some(n.to_owned())))
            .collect(),
        deal_info: [
            ("d1", "合成運輸 2026年案件", "契約中", true, "2026-03-01"),
            ("d2", "合成運輸 2025年案件", "契約終了", false, "2025-01-15"),
            ("d3", "合成介護 案件", "契約終了", false, "2024-06-01"),
        ]
        .into_iter()
        .map(|(id, name, stage, active, created)| {
            (
                id.to_owned(),
                DealInfo {
                    name: Some(name.to_owned()),
                    stage_label: Some(stage.to_owned()),
                    active,
                    created_at: Some(created.to_owned()),
                },
            )
        })
        .collect(),
    }
}

#[test]
fn digest_sums_four_weeks_and_the_four_before() {
    let value = weeks(
        [(1000, 3), (2000, 4), (0, 0), (500, 1)],
        [(100, 1), (100, 1), (100, 1), (100, 1)],
    );
    let digest = PerfDigest::from_json(&value).unwrap();
    assert_eq!(
        (digest.recent_apps, digest.recent_cost, digest.prev_apps),
        (8, 3500, 4)
    );
    assert_eq!(digest.latest_end, date("2026-10-04"));
    assert!(PerfDigest::from_json(&perf(&[])).is_none());
    // 日付が壊れた行は使わない。
    assert!(PerfDigest::from_json(&perf(&[("2026-13-40", "2026-13-41", 1, 1)])).is_none());
}

#[test]
fn cards_count_with_concrete_rules() {
    let more = weeks(
        [(0, 5), (0, 5), (0, 0), (0, 0)],
        [(0, 1), (0, 1), (0, 1), (0, 1)],
    ); // 10 vs 4
    let flat = weeks(
        [(0, 1), (0, 1), (0, 1), (0, 1)],
        [(0, 1), (0, 1), (0, 1), (0, 1)],
    ); // 4 vs 4: 同数
    let less = weeks(
        [(0, 0), (0, 0), (0, 1), (0, 0)],
        [(0, 1), (0, 1), (0, 1), (0, 0)],
    ); // 1 vs 3: 差 -2
    let rows = vec![
        listing("1", &["d1"], "掲載中", "set", Some(&more)),
        listing("2", &["d1"], "掲載中", "unset", Some(&flat)),
        listing("3", &["d3"], "掲載中", "none", Some(&less)),
        listing("4", &["d3"], "掲載終了", "unset", Some(&more)), // 掲載終了は数えない
        listing("5", &[], "掲載中", "unset", None),              // 実績なし: 増減に入らない
    ];
    let home = compute(&rows, &map(), date("2026-10-06"));
    let by = |key: &str| home.cards.iter().find(|c| c.key == key).unwrap();
    assert_eq!(by("more").count, 1);
    assert_eq!(by("more").listings[0].id, "1");
    assert_eq!(
        (
            by("more").listings[0].recent_apps,
            by("more").listings[0].prev_apps
        ),
        (Some(10), Some(4))
    );
    assert_eq!(by("more").listings[0].company.as_deref(), Some("合成運輸"));
    assert_eq!(by("flat").count, 1);
    assert_eq!(by("flat").listings[0].id, "2");
    assert_eq!(by("less").count, 1);
    assert_eq!(by("less").listings[0].id, "3");
    // 職種が未指定の掲載中: 2 と 5（4 は掲載終了、1 と 3 は指定済み）
    assert_eq!(by("occupation").count, 2);
    assert_eq!(home.live_without_company, 1);
}

#[test]
fn costly_and_stale_use_the_documented_thresholds() {
    // 応募あり: 1 件あたり 1000 円（課金 5000 / 応募 5）が基準になる中央値の材料。
    let cheap = weeks([(1000, 1), (1000, 1), (1000, 1), (2000, 2)], [(0, 0); 4]); // 5000/5 = 1000
    let cheap2 = weeks([(1000, 1), (1000, 1), (1000, 1), (1000, 1)], [(0, 0); 4]); // 4000/4 = 1000
    let pricey = weeks([(5000, 1), (5000, 0), (0, 0), (0, 0)], [(0, 0); 4]); // 10000/1 = 10000 >= 2000
    let none = weeks([(6000, 0), (6000, 0), (0, 0), (0, 0)], [(0, 0); 4]); // 12000 円・応募 0
    let small = weeks([(4000, 0), (4000, 0), (0, 0), (0, 0)], [(0, 0); 4]); // 8000 円は下限未満
    let old = perf(&[("2026-08-03", "2026-08-09", 100, 1)]); // 最新の週が 8/09 に終わった
    let rows = vec![
        listing("1", &["d1"], "掲載中", "set", Some(&cheap)),
        listing("2", &["d1"], "掲載中", "set", Some(&cheap2)),
        listing("3", &["d1"], "掲載中", "set", Some(&pricey)),
        listing("4", &["d2"], "掲載中", "set", Some(&none)),
        listing("5", &["d2"], "掲載中", "set", Some(&small)),
        listing("6", &["d3"], "掲載中", "set", Some(&old)),
    ];
    let home = compute(&rows, &map(), date("2026-10-06"));
    // 中央値は 1000, 1000, 10000 の 1000。
    assert_eq!(home.median_cost_per_app, Some(1000));
    let ids = |key: &str| {
        let card = home.cards.iter().find(|c| c.key == key).unwrap();
        card.listings
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("costly"), vec!["3", "4"]);
    // 今日 10/06 - 最新の週の終わり: 1〜5 は 10/04 の 2 日前（止まっていない）、6 は 58 日前。
    assert_eq!(ids("stale"), vec!["6"]);
    // 境界: 14 日ちょうどは止まっていない、15 日目から止まっている。
    let edge = perf(&[("2026-09-14", "2026-09-20", 0, 0)]);
    let rows = vec![listing("7", &["d1"], "掲載中", "set", Some(&edge))];
    let stale_on = |today: &str| {
        compute(&rows, &map(), date(today))
            .cards
            .iter()
            .find(|c| c.key == "stale")
            .unwrap()
            .count
    };
    assert_eq!(stale_on("2026-10-04"), 0);
    assert_eq!(stale_on("2026-10-05"), 1);
}

#[test]
fn customer_table_adds_up_live_listings_per_company() {
    let a = weeks(
        [(1000, 2), (1000, 2), (0, 0), (0, 0)],
        [(0, 1), (0, 0), (0, 0), (0, 0)],
    ); // 4 (cost 2000) vs 1
    let b = weeks(
        [(3000, 1), (0, 0), (0, 0), (0, 0)],
        [(0, 2), (0, 0), (0, 0), (0, 0)],
    ); // 1 (cost 3000) vs 2
    let c = weeks([(500, 3), (0, 0), (0, 0), (0, 0)], [(0, 0); 4]);
    let rows = vec![
        listing("1", &["d1"], "掲載中", "set", Some(&a)),
        listing("2", &["d2"], "掲載中", "set", Some(&b)),
        // 2 つの取引が同じ事業所でも、求人は 1 回だけ数える。
        listing("3", &["d1", "d2"], "掲載中", "set", Some(&c)),
        listing("4", &["d1"], "掲載終了", "set", Some(&a)),
        listing("5", &["d3"], "掲載中", "set", None),
    ];
    let home = compute(&rows, &map(), date("2026-10-06"));
    let c1 = home
        .customers
        .iter()
        .find(|c| c.company_id == "c1")
        .unwrap();
    assert_eq!(c1.name.as_deref(), Some("合成運輸"));
    assert_eq!(c1.deals, 2);
    // d1 は進行中、d2 は契約終了。求人は 4 件結び付いていて、掲載中は 3 件。
    assert_eq!(
        (c1.active_deals, c1.live_listings, c1.total_listings),
        (1, 3, 4)
    );
    let latest = c1.latest_deal.as_ref().unwrap();
    assert_eq!(latest.name.as_deref(), Some("合成運輸 2026年案件"));
    assert_eq!(latest.stage_label.as_deref(), Some("契約中"));
    assert_eq!(latest.created_at.as_deref(), Some("2026-03-01"));
    assert_eq!((c1.recent_apps, c1.prev_apps, c1.recent_cost), (8, 3, 5500));
    assert_eq!(c1.cost_per_app, Some(687));
    let c2 = home
        .customers
        .iter()
        .find(|c| c.company_id == "c2")
        .unwrap();
    assert_eq!(
        (c2.deals, c2.live_listings, c2.recent_apps, c2.cost_per_app),
        (1, 1, 0, None)
    );
    // c2 の案件は契約終了だけ（進行中 0）。
    assert_eq!((c2.active_deals, c2.total_listings), (0, 1));
    // 進行中の案件がある顧客が先。
    assert_eq!(home.customers[0].company_id, "c1");
}

#[test]
fn customers_sort_active_first_then_latest_deal_created_desc() {
    let mut m = map();
    m.deal_company.insert("d4".into(), "c3".into());
    m.deal_company.insert("d5".into(), "c4".into());
    m.names.insert("c3".into(), Some("合成建設".into()));
    m.names.insert("c4".into(), Some("合成商事".into()));
    for (id, active, created) in [("d4", true, "2026-09-01"), ("d5", true, "2026-05-01")] {
        m.deal_info.insert(
            id.into(),
            DealInfo {
                name: Some(id.into()),
                stage_label: None,
                active,
                created_at: Some(created.into()),
            },
        );
    }
    let rows = vec![
        listing("1", &["d1"], "掲載中", "set", None),
        listing("2", &["d3"], "掲載終了", "set", None),
        listing("3", &["d4"], "掲載中", "set", None),
        listing("4", &["d5"], "掲載中", "set", None),
    ];
    let home = compute(&rows, &m, date("2026-10-06"));
    let order: Vec<&str> = home
        .customers
        .iter()
        .map(|c| c.company_id.as_str())
        .collect();
    // 進行中: c3(9/1) → c1(3/1) → c4(5/1)? 作成日の新しい順なので c3, c4, c1。最後が進行中なしの c2。
    assert_eq!(order, vec!["c3", "c4", "c1", "c2"]);
}

#[test]
fn current_deal_prefers_active_over_a_newer_ended_one_and_falls_back_to_the_newest() {
    let mut m = map();
    // c1: d1 は契約中（2026-03-01）、d2 は契約終了だが新しい（2026-09-30）にする。
    m.deal_info.get_mut("d2").unwrap().created_at = Some("2026-09-30".into());
    let rows = vec![
        listing("1", &["d1"], "掲載中", "set", None),
        listing("2", &["d2"], "掲載終了", "set", None),
        listing("3", &["d3"], "掲載終了", "set", None),
    ];
    let home = compute(&rows, &m, date("2026-10-06"));
    let c1 = home
        .customers
        .iter()
        .find(|c| c.company_id == "c1")
        .unwrap();
    assert_eq!(
        c1.latest_deal.as_ref().unwrap().name.as_deref(),
        Some("合成運輸 2026年案件")
    );
    // 同じ求人が 2 つの取引に付いていても、求人は 1 回だけ数える。
    let rows = vec![listing("9", &["d1", "d2"], "掲載中", "set", None)];
    let home = compute(&rows, &m, date("2026-10-06"));
    let c1 = home
        .customers
        .iter()
        .find(|c| c.company_id == "c1")
        .unwrap();
    assert_eq!((c1.deals, c1.live_listings, c1.total_listings), (2, 1, 1));
    // c2 は契約中が無い → 最新の取引（d3 だけ）。
    let rows = vec![listing("3", &["d3"], "掲載終了", "set", None)];
    let home = compute(&rows, &m, date("2026-10-06"));
    let c2 = home
        .customers
        .iter()
        .find(|c| c.company_id == "c2")
        .unwrap();
    assert_eq!(
        (
            c2.active_deals,
            c2.latest_deal.as_ref().unwrap().created_at.as_deref()
        ),
        (0, Some("2024-06-01"))
    );
}

#[test]
fn only_a_single_company_deal_is_linked_and_lists_are_capped() {
    let value = weeks([(0, 9), (0, 0), (0, 0), (0, 0)], [(0, 0); 4]);
    let rows: Vec<Listing> = (0..130)
        .map(|i| listing(&i.to_string(), &["d1"], "掲載中", "set", Some(&value)))
        .collect();
    let home = compute(&rows, &map(), date("2026-10-06"));
    let more = home.cards.iter().find(|c| c.key == "more").unwrap();
    assert_eq!(more.count, 130);
    assert_eq!(more.listings.len(), LIST_CAP);
}

#[test]
fn trend_examples_belong_to_exactly_one_card() {
    for (prev, recent, expected) in [
        (0, 0, "zero_stay"),
        (1, 0, "to_zero"),
        (5, 0, "to_zero"),
        (2, 1, "less"),
        (3, 2, "less"),
        (5, 4, "less"),
        (10, 8, "less"),
        (10, 9, "flat"),
        (0, 1, "more"),
        (4, 5, "more"),
        (10, 11, "flat"),
        (10, 12, "more"),
        (5, 5, "flat"),
        (101, 100, "flat"),
    ] {
        let value = weeks(
            [(0, recent), (0, 0), (0, 0), (0, 0)],
            [(0, prev), (0, 0), (0, 0), (0, 0)],
        );
        let rows = [listing("example", &[], "掲載中", "set", Some(&value))];
        let home = compute(&rows, &map(), date("2026-10-06"));
        for card in home.cards.iter().take(5) {
            assert_eq!(
                card.count,
                usize::from(card.key == expected),
                "{prev} → {recent}: {}",
                card.key
            );
            assert_eq!(
                card.listings
                    .iter()
                    .map(|r| r.id.as_str())
                    .collect::<Vec<_>>(),
                if card.key == expected {
                    vec!["example"]
                } else {
                    vec![]
                }
            );
        }
        for status in ["掲載終了", "下書き"] {
            let rows = [listing("excluded", &[], status, "set", Some(&value))];
            assert!(compute(&rows, &map(), date("2026-10-06"))
                .cards
                .iter()
                .take(5)
                .all(|c| c.count == 0));
        }
    }
    let rows = [listing("no-performance", &[], "掲載中", "set", None)];
    assert!(compute(&rows, &map(), date("2026-10-06"))
        .cards
        .iter()
        .take(5)
        .all(|c| c.count == 0));
}

#[test]
fn zero_stay_is_first_and_sorted_by_recent_cost_before_capping() {
    let rows: Vec<_> = (0..130)
        .map(|i| {
            let value = weeks([(i * 1000, 0), (0, 0), (0, 0), (0, 0)], [(999999, 0); 4]);
            listing(&i.to_string(), &[], "掲載中", "set", Some(&value))
        })
        .collect();
    let home = compute(&rows, &map(), date("2026-10-06"));
    assert_eq!(
        home.cards.iter().map(|c| c.key).collect::<Vec<_>>(),
        vec![
            "zero_stay",
            "to_zero",
            "less",
            "more",
            "flat",
            "costly",
            "stale",
            "occupation"
        ]
    );
    let card = &home.cards[0];
    assert_eq!(card.count, 130);
    assert_eq!(card.listings.len(), LIST_CAP);
    for (row, i) in card.listings.iter().zip((30..130).rev()) {
        assert_eq!(row.id, i.to_string());
        assert_eq!(row.recent_cost, Some(i * 1000));
        assert_eq!((row.prev_apps, row.recent_apps), (Some(0), Some(0)));
    }
}

#[test]
fn trend_percentage_multiplication_handles_large_integer_counts() {
    let perf = PerfDigest {
        latest_end: date("2026-10-04"),
        recent_apps: i64::MAX - 1,
        recent_cost: 0,
        prev_apps: i64::MAX,
    };
    assert_eq!(trend_key(&perf), "flat");
    assert_eq!(
        trend_key(&PerfDigest {
            recent_apps: 1,
            ..perf
        }),
        "less"
    );
}

#[test]
fn owner_scope_includes_any_matching_deal_and_only_its_customer_links() {
    let mut snapshot = owner_todo::Snapshot::default();
    for (id, owner) in [("d1", "11"), ("d2", "11"), ("d3", "22")] {
        let record = Record {
            id: id.into(),
            properties: [
                ("consultant".into(), Some(owner.into())),
                ("hubspot_owner_id".into(), Some("22".into())),
            ]
            .into_iter()
            .collect(),
        };
        snapshot.deals.insert(
            id.into(),
            owner_todo::WorkDeal {
                assignment: owner_todo::Assignment::from_record(&record),
                record,
            },
        );
    }
    let rows = vec![
        listing("1", &["d1", "d3"], "掲載中", "unset", None),
        listing("2", &["d3"], "掲載中", "unset", None),
        listing("3", &[], "掲載中", "unset", None),
    ];
    let (selected, map) = scoped(&rows, &map(), &snapshot, "consultant", Some("11"));
    assert_eq!(
        selected.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["1"]
    );
    assert_eq!(selected[0].deal_ids, ["d1"]);
    let home = compute(&selected, &map, date("2026-10-16"));
    assert_eq!(
        home.cards
            .iter()
            .find(|c| c.key == "occupation")
            .unwrap()
            .count,
        1
    );
    assert_eq!(home.customers.len(), 1);
    assert_eq!(home.customers[0].company_id, "c1");
    assert_eq!(home.customers[0].deals, 1);
    assert_eq!(home.live_without_company, 0);
    let (selected, _) = scoped(&rows, &map, &snapshot, "hubspot_owner_id", Some("22"));
    assert_eq!(selected.len(), 2);
    let (selected, _) = scoped(&rows, &map, &snapshot, "consultant", None);
    assert_eq!(selected.len(), 3);
}
