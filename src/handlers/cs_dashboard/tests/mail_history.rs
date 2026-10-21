use super::*;
use crate::handlers::cs_dashboard::{deal_detail::build_deal_detail, empty_history_sheet};

fn fixture() -> Sheets {
    let empty = empty_history_sheet();
    Sheets {
        pending_recordings: None,
        deal: tiny(
            &[
                "deal_id",
                "dealname",
                "contract_start_date",
                "contract_expiration_date",
                "kyoten_key",
                "is_active",
            ],
            &[
                &[
                    "a",
                    "架空の前契約",
                    "2026-01-01",
                    "2026-06-30",
                    "fictional-site",
                    "TRUE",
                ],
                &[
                    "b",
                    "架空の継続契約",
                    "2026-07-01",
                    "2026-12-31",
                    "fictional-site",
                    "TRUE",
                ],
            ],
        ),
        call: tiny(
            &["call_id", "ts", "duration_sec", "deal_id"],
            &[&["fictional-call", "2026-03-01T01:00:00Z", "120", "a"]],
        ),
        mtg: tiny(
            &["deal_id", "開催日", "開催日時(JST)", "確度"],
            &[&["a", "2026-03-10", "2026-03-10 09:00", "高"]],
        ),
        mail_mtg: tiny(
            &["deal_id", "date", "kind"],
            &[
                &["a", "2026-03-10", "実施"],
                &["a", "2026-03-11", "実施"],
                &["a", "2026-03-12", "実施"],
            ],
        ),
        mail_history: tab(include_str!(
            "../../../../tests/fixtures/cs_dashboard/CS_メール履歴.tsv"
        )),
        zoom_mtg: tab(include_str!(
            "../../../../tests/fixtures/cs_dashboard/CS_MTG実施日_Zoom由来.tsv"
        )),
        history: empty.clone(),
        customer: empty.clone(),
        handover: empty.clone(),
        owner_hist: empty.clone(),
        meta: empty,
        all_cached: false,
    }
}
fn tab(s: &str) -> Arc<SheetData> {
    Arc::new(crate::handlers::sales_kpi::fixture::sheet_from_tsv(s))
}
fn detail(sh: &Sheets) -> Value {
    build_deal_detail(
        sh,
        None,
        Some("a"),
        None,
        chrono::NaiveDate::from_ymd_opt(2026, 3, 31).unwrap(),
    )
}

#[test]
fn mail_history_order_labels_attach_and_zoom_priority() {
    let sh = fixture();
    let v = detail(&sh);
    let email = events_of(&v, "email");
    assert_eq!(
        email
            .iter()
            .map(|e| (
                e["date"].as_str().unwrap(),
                e["time"].as_str().unwrap(),
                e["direction"].as_str().unwrap(),
                e["mail_type"].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("2026-03-30", "18:30", "送った", "応募の報告"),
            ("2026-03-12", "17:00", "受けた", "MTG取り下げ"),
            ("2026-03-12", "16:00", "送った", "MTG候補"),
            ("2026-03-11", "15:00", "受けた", "MTG予定"),
            ("2026-03-11", "08:00", "送った", "MTG実施"),
        ]
    );
    assert_eq!(email[4]["subject"], "架空の打ち合わせ報告");
    assert_eq!(email[4]["attach"]["state"], "moved_in");
    assert_eq!(email[4]["attach"]["moved_from"]["deal_id"], "b");
    assert_eq!(email[4]["zoom_held_time"], "10:00");
    assert!(email[3]["zoom_held_time"].is_null()); // 会議番号だけでは開催扱いにしない
    assert_eq!(v["counts"]["email"], 5);
    assert_eq!(v["counts"]["email_kinds"].as_array().unwrap().len(), 5);
    let all_zoom = events_of(&v, "zoom_mtg");
    assert_eq!(all_zoom.len(), 3);
    assert_eq!(all_zoom[0]["date"], "2026-03-29");
    assert_eq!(all_zoom[0]["period"], "契約の間");
    assert_eq!(all_zoom[0]["attach"]["in_span"], false);
    assert_eq!(all_zoom[2]["date"], "2025-12-20");
    assert_eq!(all_zoom[2]["period"], "契約前");
    assert_eq!(all_zoom[2]["attach"]["state"], "outside");
    assert_eq!(v["counts"]["zoom_outside"], 2);
    let zoom: Vec<_> = all_zoom
        .into_iter()
        .filter(|e| e["period"].is_null())
        .collect();
    assert_eq!(zoom.len(), 1); // 録画の日と同じ行・別契約に付いた重複を除く
    assert_eq!(zoom[0]["date"], "2026-03-11");
    assert_eq!(zoom[0]["time"], "10:00");
    assert_eq!(zoom[0]["end_time"], "11:00");
    assert_eq!(zoom[0]["fact"], true);
    assert_eq!(zoom[0]["attach"]["state"], "own");
    let mut moved = fixture();
    let mut zoom_sheet = (*moved.zoom_mtg).clone();
    zoom_sheet.rows.retain(|r| {
        zoom_sheet
            .header
            .iter()
            .position(|h| h == "deal_id")
            .and_then(|i| r.get(i))
            .is_none_or(|id| id.as_ref() != "a")
    });
    moved.zoom_mtg = Arc::new(zoom_sheet);
    assert_eq!(
        events_of(&detail(&moved), "zoom_mtg")[0]["attach"]["state"],
        "moved_in"
    );
    assert_eq!(events_of(&v, "mtg").len(), 1);
    assert_eq!(
        events_of(&v, "mail_mtg")
            .iter()
            .map(|e| e["date"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["2026-03-12"]
    );
    // E2Eが使うJSONは、このRustの出力と具体値まで一致させる。
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cs_dashboard/deal-mail-history.json"
    );
    if std::env::var_os("UPDATE_MAIL_HISTORY_FIXTURE").is_some() {
        std::fs::write(path, serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
    }
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(v, expected);
}

#[test]
fn mail_does_not_count_as_contact_but_zoom_is_factual_mtg() {
    let mut sh = fixture();
    let (contacts, calls, meetings) = super::super::contacts_by_deal(&sh.call, &sh.factual_mtg());
    assert_eq!((calls, meetings), (1, 3)); // 2取引の原記録。aは録画1＋Zoom1、bは付け直し元1
    assert_eq!(
        contacts["a"].iter().max().unwrap().to_string(),
        "2026-03-11"
    );
    let before_contacts = contacts;
    sh.mail_history = empty_history_sheet();
    assert_eq!(
        super::super::contacts_by_deal(&sh.call, &sh.factual_mtg()).0,
        before_contacts
    );
    let last = super::super::last_mtg_by_deal(&sh.factual_mtg(), &empty_history_sheet());
    assert_eq!(last["a"].0.unwrap().to_string(), "2026-03-11");
    assert!(last["a"].1.is_none());
    let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
    let (rows, _) = super::super::routes::deal_rows_uncached(&sh, today);
    let row = rows.iter().find(|r| r["deal_id"] == "a").unwrap();
    assert_eq!(row["days_since_contact"], 20);
    // メール推定が3/12なので、MTGの日だけは推定の方が新しい。
    assert_eq!(row["mtg_last"], "2026-03-12");
    sh.mail_mtg = empty_history_sheet();
    let (rows, _) = super::super::routes::deal_rows_uncached(&sh, today);
    let row = rows.iter().find(|r| r["deal_id"] == "a").unwrap();
    assert_eq!(row["mtg_last"], "2026-03-11");
    assert_eq!(row["mtg_days"], 20);
    assert_eq!(row["mtg_source_label"], "Zoom 開催記録（事実）");
}

#[test]
fn missing_additional_tabs_preserve_existing_events_and_contacts() {
    let mut sh = fixture();
    sh.mail_history = empty_history_sheet();
    sh.zoom_mtg = empty_history_sheet();
    let v = detail(&sh);
    assert_eq!(events_of(&v, "email").len(), 0);
    assert_eq!(events_of(&v, "zoom_mtg").len(), 0);
    assert_eq!(
        events_of(&v, "mail_mtg")
            .iter()
            .map(|e| e["date"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["2026-03-12", "2026-03-11", "2026-03-10"]
    );
    assert_eq!(
        super::super::contacts_by_deal(&sh.call, &sh.mtg),
        super::super::contacts_by_deal(&sh.call, &sh.factual_mtg())
    );
}

#[test]
fn zoom_changes_flags_using_the_production_aggregation() {
    let mut sh = sheets();
    let before = super::super::routes::deal_rows_uncached(&sh, fixture_day()).0;
    let target = before.iter().find(|r| r["mtg_band"] == "critical").unwrap()["deal_id"]
        .as_str()
        .unwrap()
        .to_owned();
    sh.zoom_mtg = tiny(
        &["deal_id", "date", "kind"],
        &[&[&target, "2026-09-17", "実施"]],
    );
    let after = super::super::routes::deal_rows_uncached(&sh, fixture_day()).0;
    for period in ["契約前", "契約の間"] {
        sh.zoom_mtg = tiny(
            &["deal_id", "date", "kind", "period"],
            &[&[&target, "2026-09-17", "実施", period]],
        );
        assert_eq!(
            super::super::routes::deal_rows_uncached(&sh, fixture_day()).0,
            before
        );
    }
    let tally = |rows: &[Value]| {
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        for r in rows {
            for flag in r["flags"].as_array().unwrap() {
                *counts.entry(flag.as_str().unwrap().into()).or_default() += 1;
            }
        }
        counts
    };
    let old = tally(&before);
    let new = tally(&after);
    let changed: Vec<_> = old
        .iter()
        .filter_map(|(k, v)| {
            let n = new.get(k).copied().unwrap_or(0);
            (*v != n).then_some((k.clone(), *v, n))
        })
        .collect();
    eprintln!("Zoom追加による名札の差分（架空の開催1件、既存fixture全604案件）: {changed:?}");
    assert_eq!(
        changed,
        vec![
            ("MTGが60日以上空いている".into(), 77, 76),
            ("接触が30日以上空いている".into(), 69, 68),
        ]
    );
    assert_eq!(
        after.iter().find(|r| r["deal_id"] == target).unwrap()["mtg_last"],
        "2026-09-17"
    );
}

#[test]
fn application_report_alone_leaves_contact_missing() {
    let mut sh = fixture();
    sh.call = empty_history_sheet();
    sh.mtg = empty_history_sheet();
    sh.zoom_mtg = empty_history_sheet();
    let (contacts, calls, meetings) = super::super::contacts_by_deal(&sh.call, &sh.factual_mtg());
    assert_eq!((contacts.len(), calls, meetings), (0, 0, 0));
    let (rows, _) = super::super::routes::deal_rows_uncached(
        &sh,
        chrono::NaiveDate::from_ymd_opt(2026, 3, 31).unwrap(),
    );
    let row = rows.iter().find(|r| r["deal_id"] == "a").unwrap();
    assert_eq!(row["n_contact"], 0);
    assert!(row["days_since_contact"].is_null());
    assert!(row["flags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == "接触の記録が無い"));
    assert_eq!(events_of(&detail(&sh), "email").len(), 5);
}

#[test]
fn outside_zoom_preserves_all_flags_and_legacy_sheet_counts_every_row() {
    let mut sh = fixture();
    sh.mail_mtg = empty_history_sheet();
    let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
    let before = super::super::routes::deal_rows_uncached(&sh, today).0;
    let mut zoom = (*sh.zoom_mtg).clone();
    zoom.rows
        .retain(|r| super::super::zoom_in_contract(&sh.zoom_mtg, r));
    sh.zoom_mtg = Arc::new(zoom);
    assert_eq!(
        before,
        super::super::routes::deal_rows_uncached(&sh, today).0
    );
    // 日付だけでは期間内に見える行も period を優先して除外する。
    let mut sh = fixture();
    sh.mail_mtg = empty_history_sheet();
    let mut legacy = (*sh.zoom_mtg).clone();
    let column = legacy.header.iter().position(|h| h == "period").unwrap();
    legacy.header.remove(column);
    for row in &mut legacy.rows {
        row.remove(column);
    }
    sh.zoom_mtg = Arc::new(legacy);
    let rows = super::super::routes::deal_rows_uncached(&sh, today).0;
    let row = rows.iter().find(|r| r["deal_id"] == "a").unwrap();
    assert_eq!(row["mtg_last"], "2026-03-29");
    assert_eq!(row["mtg_days"], 2);
    assert_eq!(row["days_since_contact"], 2);
}

#[test]
fn email_dedupe_prefers_direct_then_main_with_or_without_mail_id() {
    for with_id in [false, true] {
        let mut sh = fixture();
        sh.deal = tab("deal_id\tdealname\tcontract_start_date\tcontract_expiration_date\tkyoten_key\tis_active\tcontract_kind\n\
a\t架空の前契約\t2026-01-01\t2026-06-30\tfictional-site\tTRUE\t\n\
b\t架空の継続契約\t2026-07-01\t2026-12-31\tfictional-site\tTRUE\t\n\
c\t架空の求人追加\t2026-01-01\t2026-06-30\tfictional-site\tTRUE\t求人追加\n");
        let header = "deal_id\t送信日時（JST）\t向き\t件名\t種類\tzoom_meeting_id\tzoom_held";
        let mut tsv = format!("{header}{}\n", if with_id { "\tmail_id" } else { "" });
        for did in ["c", "b", "a"] {
            tsv += &format!(
                "{did}\t2026-03-11 08:00:00\t送った\t架空の報告\tMTG実施\t\t{}\n",
                if with_id { "\tfictional-mail" } else { "" }
            );
        }
        sh.mail_history = tab(&tsv);
        let v = detail(&sh);
        assert_eq!(v["counts"]["email"], 1);
        assert_eq!(v["counts"]["email_moved_in"], 0);
        assert_eq!(events_of(&v, "email")[0]["attach"]["state"], "own");
        // 直接の行が無ければ、先に並ぶオプションより本体契約を採る。
        let without_direct = tsv
            .lines()
            .filter(|line| !line.starts_with("a\t"))
            .collect::<Vec<_>>()
            .join("\n");
        sh.mail_history = tab(&without_direct);
        let v = detail(&sh);
        assert_eq!(v["counts"]["email"], 1);
        assert_eq!(v["counts"]["email_moved_in"], 1);
        assert_eq!(
            events_of(&v, "email")[0]["attach"]["moved_from"]["deal_id"],
            "b"
        );
        if with_id {
            // 同じ日時・件名でもIDが異なるメールは別の2件。
            tsv += "a\t2026-03-11 08:00:00\t送った\t架空の報告\tMTG実施\t\t\tfictional-other\n";
            sh.mail_history = tab(&tsv);
            assert_eq!(detail(&sh)["counts"]["email"], 2);
        }
    }
}
