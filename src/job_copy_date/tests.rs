use super::*;

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}
fn time(value: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(value).unwrap()
}
fn observation(id: &str, version: &str, observed: &str) -> Observation {
    let observed_at = time(observed);
    Observation {
        observation_id: id.to_owned(),
        version_id: version.to_owned(),
        source_generated_date: Some(date_in_jst(&observed_at)),
        observed_at,
        quality_valid: true,
    }
}
fn application(id: &str, day: &str, gender: Option<&str>) -> ApplicantRecord {
    ApplicantRecord {
        application_id: id.to_owned(),
        application_date: ApplicationDate::DateOnly { date: date(day) },
        listing_unambiguous: true,
        attributes: ApplicantAttributes {
            gender: gender.map(str::to_owned),
            ..Default::default()
        },
    }
}

#[test]
fn date_only_is_not_shifted_and_only_real_timestamps_are_converted_to_jst() {
    assert_eq!(
        ApplicationDate::DateOnly {
            date: date("2026-10-03")
        }
        .jst_date(),
        Some(date("2026-10-03"))
    );
    assert_eq!(
        ApplicationDate::RealTimestamp {
            timestamp: time("2026-10-03T15:00:00Z")
        }
        .jst_date(),
        Some(date("2026-10-04"))
    );
    assert_eq!(
        ApplicationDate::RealTimestamp {
            timestamp: time("2026-10-03T14:59:59Z")
        }
        .jst_date(),
        Some(date("2026-10-03"))
    );
    assert_eq!(
        ApplicationDate::SyntheticTimestamp {
            timestamp: time("2026-10-03T15:00:00Z")
        }
        .jst_date(),
        None
    );
}

#[test]
fn last_valid_observation_wins_without_erasing_same_day_return_events() {
    let observations = [
        observation("a-early", "A", "2026-10-03T08:00:00+09:00"),
        observation("b", "B", "2026-10-03T12:00:00+09:00"),
        observation("a-return", "A-return", "2026-10-03T18:00:00+09:00"),
    ];
    let index = build_daily_index(&observations, date("2026-10-03"));
    assert_eq!(
        index
            .observations
            .iter()
            .map(|item| item.version_id.as_str())
            .collect::<Vec<_>>(),
        ["A", "B", "A-return"]
    );
    let representative = index.representatives.get(&date("2026-10-03")).unwrap();
    assert_eq!(representative.version_id, "A-return");
    assert!(representative.provisional);
    let applications = [
        application("morning-date-only", "2026-10-03", None),
        application("evening-date-only", "2026-10-03", Some("女性")),
    ];
    let result = aggregate_applications(&index, Some(&applications)).unwrap();
    assert_eq!(result.per_version.get("A-return"), Some(&2));
    assert!(!result.per_version.contains_key("B"));
}

#[test]
fn invalid_late_capture_and_stale_export_do_not_replace_a_valid_body_observation() {
    let good = observation("good", "A", "2026-10-03T08:00:00+09:00");
    let mut invalid = observation("http-success-bad-body", "B", "2026-10-03T18:00:00+09:00");
    invalid.quality_valid = false;
    let mut stale = observation("reused-old-export", "C", "2026-10-04T18:00:00+09:00");
    stale.source_generated_date = Some(date("2026-10-03"));
    let index = build_daily_index(&[good, invalid, stale], date("2026-10-05"));
    assert_eq!(
        index
            .representatives
            .get(&date("2026-10-03"))
            .unwrap()
            .version_id,
        "A"
    );
    assert!(!index.representatives.contains_key(&date("2026-10-04")));
    assert_eq!(
        index
            .excluded
            .iter()
            .map(|item| item.reason)
            .collect::<Vec<_>>(),
        [
            ObservationExclusion::QualityInvalid,
            ObservationExclusion::StaleExport
        ]
    );
}

#[test]
fn missing_observation_day_is_not_filled_from_either_neighbor_and_totals_reconcile() {
    let index = build_daily_index(
        &[
            observation("first", "A", "2026-10-03T10:00:00+09:00"),
            observation("last", "B", "2026-10-05T10:00:00+09:00"),
        ],
        date("2026-10-06"),
    );
    let applications = [
        application("a1", "2026-10-03", Some("男性")),
        application("a2", "2026-10-03", None),
        application("b1", "2026-10-05", Some("女性")),
        application("gap", "2026-10-04", None),
        application("before-first", "2026-10-02", None),
    ];
    let result = aggregate_applications(&index, Some(&applications)).unwrap();
    assert_eq!(result.total_unique_records, 5);
    assert_eq!(result.per_version.get("A"), Some(&2));
    assert_eq!(result.per_version.get("B"), Some(&1));
    assert_eq!(result.unknown_records, 2);
    assert_eq!(
        result.per_version.values().sum::<usize>() + result.unknown_records,
        result.total_unique_records
    );
    let distribution = distribution_for_version(
        Some(&applications),
        &result,
        "A",
        AttributeDimension::Gender,
    )
    .unwrap();
    assert_eq!(distribution.denominator, 2);
    assert_eq!(
        distribution
            .categories
            .iter()
            .find(|row| row.category == "不明")
            .unwrap()
            .percentage,
        Some(50.0)
    );
    assert_eq!(
        distribution
            .categories
            .iter()
            .map(|row| row.count)
            .sum::<usize>(),
        2
    );
}

#[test]
fn unknown_freshness_remains_explicit_and_today_is_provisional_only() {
    let mut unknown = observation("unknown-source-age", "A", "2026-10-03T10:00:00+09:00");
    unknown.source_generated_date = None;
    let index = build_daily_index(&[unknown], date("2026-10-04"));
    let day = index.representatives.get(&date("2026-10-03")).unwrap();
    assert_eq!(day.source_freshness, SourceFreshness::Unverified);
    assert!(!day.provisional);
    let result =
        aggregate_applications(&index, Some(&[application("one", "2026-10-03", None)])).unwrap();
    assert_eq!(
        result.attributions[0].source_freshness,
        Some(SourceFreshness::Unverified)
    );
}

#[test]
fn identical_duplicate_records_count_once_and_conflicting_duplicates_stay_unknown() {
    let index = build_daily_index(
        &[observation("first", "A", "2026-10-03T10:00:00+09:00")],
        date("2026-10-04"),
    );
    let first = application("one", "2026-10-03", Some("女性"));
    let second = application("two", "2026-10-03", Some("男性"));
    let mut conflicting = second.clone();
    conflicting.application_date = ApplicationDate::DateOnly {
        date: date("2026-10-04"),
    };
    let result =
        aggregate_applications(&index, Some(&[first.clone(), first, second, conflicting])).unwrap();
    assert_eq!(result.total_unique_records, 2);
    assert_eq!(result.duplicate_rows, 2);
    assert_eq!(result.per_version.get("A"), Some(&1));
    assert_eq!(result.unknown_records, 1);
    assert_eq!(
        result
            .attributions
            .iter()
            .find(|row| row.application_id == "two")
            .unwrap()
            .unknown_reason,
        Some(UnknownAttributionReason::ConflictingDuplicateRecord)
    );
}

#[test]
fn synthetic_date_ambiguous_mapping_and_missing_date_are_not_ordinary_date_only_failures() {
    let index = build_daily_index(
        &[observation("first", "A", "2026-10-03T10:00:00+09:00")],
        date("2026-10-04"),
    );
    let mut synthetic = application("synthetic", "2026-10-03", None);
    synthetic.application_date = ApplicationDate::SyntheticTimestamp {
        timestamp: time("2026-10-03T01:00:00Z"),
    };
    let mut ambiguous = application("ambiguous", "2026-10-03", None);
    ambiguous.listing_unambiguous = false;
    let mut missing = application("missing", "2026-10-03", None);
    missing.application_date = ApplicationDate::Unknown;
    let result = aggregate_applications(&index, Some(&[synthetic, ambiguous, missing])).unwrap();
    assert_eq!(result.unknown_records, 3);
    let reasons: BTreeSet<_> = result
        .attributions
        .iter()
        .filter_map(|row| row.unknown_reason)
        .map(|reason| format!("{reason:?}"))
        .collect();
    assert!(reasons.contains("SyntheticApplicationTimestamp"));
    assert!(reasons.contains("AmbiguousListing"));
    assert!(reasons.contains("MissingApplicationDate"));
}

#[test]
fn unavailable_application_source_is_not_an_acquired_zero_record_set() {
    let index = build_daily_index(&[], date("2026-10-04"));
    assert_eq!(aggregate_applications(&index, None), None);
    let empty = aggregate_applications(&index, Some(&[])).unwrap();
    assert_eq!(empty.total_unique_records, 0);
    assert_eq!(
        distribution_for_version(None, &empty, "A", AttributeDimension::Age),
        None
    );
    let distribution =
        distribution_for_version(Some(&[]), &empty, "A", AttributeDimension::Age).unwrap();
    assert_eq!(distribution.denominator, 0);
    assert!(distribution.categories.is_empty());
}

#[test]
fn a_partial_attribute_snapshot_is_not_reported_as_zero_or_a_smaller_denominator() {
    let index = build_daily_index(
        &[observation("first", "A", "2026-10-03T10:00:00+09:00")],
        date("2026-10-04"),
    );
    let applications = [
        application("one", "2026-10-03", None),
        application("two", "2026-10-03", Some("女性")),
    ];
    let result = aggregate_applications(&index, Some(&applications)).unwrap();
    assert!(distribution_for_version(
        Some(&applications[..1]),
        &result,
        "A",
        AttributeDimension::Gender
    )
    .is_none());
}

#[test]
fn final_timestamp_conflict_is_not_resolved_by_input_order() {
    let index = build_daily_index(
        &[
            observation("one", "A", "2026-10-03T10:00:00+09:00"),
            observation("two", "B", "2026-10-03T10:00:00+09:00"),
        ],
        date("2026-10-04"),
    );
    assert!(index.representatives.is_empty());
    let result =
        aggregate_applications(&index, Some(&[application("app", "2026-10-03", None)])).unwrap();
    assert_eq!(
        result.attributions[0].unknown_reason,
        Some(UnknownAttributionReason::ConflictingDailyObservations)
    );
}

#[test]
fn municipal_names_include_prefecture_and_missing_attributes_remain_in_denominator() {
    let index = build_daily_index(
        &[observation("first", "A", "2026-10-03T10:00:00+09:00")],
        date("2026-10-04"),
    );
    let mut first = application("one", "2026-10-03", None);
    first.attributes.prefecture = Some("東京都".to_owned());
    first.attributes.municipality = Some("府中市".to_owned());
    first.attributes.age = Some(29);
    let mut second = application("two", "2026-10-03", None);
    second.attributes.prefecture = Some("広島県".to_owned());
    second.attributes.municipality = Some("府中市".to_owned());
    second.attributes.age = Some(30);
    let applications = [first, second, application("missing", "2026-10-03", None)];
    let result = aggregate_applications(&index, Some(&applications)).unwrap();
    let distribution = distribution_for_version(
        Some(&applications),
        &result,
        "A",
        AttributeDimension::Municipality,
    )
    .unwrap();
    assert_eq!(distribution.denominator, 3);
    assert_eq!(
        distribution
            .categories
            .iter()
            .map(|row| row.category.as_str())
            .collect::<Vec<_>>(),
        ["不明", "広島県 / 府中市", "東京都 / 府中市"]
    );
    let ages = distribution_for_version(Some(&applications), &result, "A", AttributeDimension::Age)
        .unwrap();
    assert_eq!(
        ages.categories
            .iter()
            .map(|row| row.category.as_str())
            .collect::<Vec<_>>(),
        ["20代", "30代", "不明"]
    );
}
