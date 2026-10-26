//! 初回の立ち上げの関門（onboarding.rs）を、実データの fixture と合成した応募・掲載のデータで確かめる。
use super::{fixture_day, sheets};
use crate::handlers::cs_dashboard::routes::{annotate_shared, build_today_board_with, deal_rows};
use crate::handlers::cs_dashboard::{date10, Sheets};
use crate::handlers::job_copy_live::shared_applications::{DealApplications, Published};
use serde_json::Value;

/// 案件管理の応募・掲載のデータを作る。稼働中の案件を3種類に分ける:
/// 3つに1つは「掲載も応募も無い」（開始から14日・30日を過ぎていれば遅れ）、
/// 3つに1つは「掲載中で、開始日に応募あり」、残りはその案件の分が無い（不明）
fn onboarding_facts(sheets: &Sheets, day: chrono::NaiveDate) -> Published {
    let (rows, _) = deal_rows(sheets, day);
    let mut deals = std::collections::BTreeMap::new();
    for (k, r) in rows.iter().enumerate() {
        let id = r["deal_id"].as_str().unwrap().to_string();
        let start = r["start"].as_str().and_then(date10);
        match k % 3 {
            0 => {
                deals.insert(
                    id,
                    DealApplications {
                        days: vec![],
                        unknown_dates: 0,
                        live: 0,
                    },
                );
            }
            1 => {
                deals.insert(
                    id,
                    DealApplications {
                        days: start.into_iter().collect(),
                        unknown_dates: 0,
                        live: 1,
                    },
                );
            }
            _ => {}
        }
    }
    Published {
        built_at: "2026-09-18T03:00:00Z".parse().unwrap(),
        deals,
    }
}

fn ids_of(rs: &[Value]) -> Vec<String> {
    rs.iter()
        .map(|r| r["deal_id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn 初回の関門は初回で開始済みの稼働中だけに付き_データが無いものは不明() {
    let sh = sheets();
    let day = fixture_day();
    let facts = onboarding_facts(&sh, day);
    let (mut rows, mut meta) = deal_rows(&sh, day);
    annotate_shared(&mut rows, &mut meta, day, Some(&facts));
    let mut n_initial = 0;
    for r in &rows {
        let started = r["start"]
            .as_str()
            .and_then(date10)
            .is_some_and(|s| s <= day);
        let first = r["renewal_no"] == 0;
        let overdue = r["phase_key"] == "overdue";
        let expect = first && started && !overdue;
        assert_eq!(!r["onboarding"].is_null(), expect, "対象の判定: {r}");
        if expect {
            n_initial += 1;
            let st = r["onboarding"]["states"].as_array().unwrap();
            assert_eq!(st.len(), 5);
            // 面接の項目が空なら不明
            if r["interviews"].is_null() {
                assert_eq!(st[3], "unknown", "{r}");
            }
            // 案件管理のデータが無い案件は掲載・応募が不明（遅れにしない）
            if !facts.deals.contains_key(r["deal_id"].as_str().unwrap()) {
                assert_eq!(st[1], "unknown");
                assert_eq!(st[2], "unknown");
            }
        }
    }
    assert_eq!(meta["onboarding"]["n_initial"], n_initial);
    assert_eq!(n_initial, 244);
    // 合成データでの関門ごとの件数: [遅れ, 不明]。面接の不明 15 は取引の項目が空のもの
    let g = &meta["onboarding"]["gates"];
    let lu = |k: usize| {
        (
            g[k]["late"].as_u64().unwrap(),
            g[k]["unknown"].as_u64().unwrap(),
        )
    };
    assert_eq!(lu(0), (12, 0));
    assert_eq!(lu(1), (77, 84));
    assert_eq!(lu(2), (60, 84));
    assert_eq!(lu(3).1, 15);
    assert_eq!(meta["onboarding"]["n_late_any"], 111);
    eprintln!(
        "ONBOARDING n_initial={n_initial} late_any={} gates={}",
        meta["onboarding"]["n_late_any"], meta["onboarding"]["gates"]
    );
}

#[test]
fn 今日の候補に初回の遅れが重ならずに入り_既存の数え方は変わらない() {
    let sh = sheets();
    let day = fixture_day();
    let facts = onboarding_facts(&sh, day);
    let before = build_today_board_with(&sh, day, None);
    let after = build_today_board_with(&sh, day, Some(&facts));

    // 上位24件・満足度4以下の追加・候補の数え方は変えない
    assert_eq!(
        ids_of(before["rows"].as_array().unwrap()),
        ids_of(after["rows"].as_array().unwrap())
    );
    assert_eq!(
        ids_of(before["extra_low_satisfaction"].as_array().unwrap()),
        ids_of(after["extra_low_satisfaction"].as_array().unwrap())
    );
    for k in ["n_hit", "n_flags2", "n_low_only", "n_extra_low", "keep"] {
        assert_eq!(before["meta"][k], after["meta"][k], "{k}");
    }
    // 案件管理のデータが無くても、MTG・面接・満足度の遅れは出る。応募・掲載のデータが加わると遅れが増える
    let n_before = before["extra_onboarding_late"].as_array().unwrap().len();
    let n_after = after["extra_onboarding_late"].as_array().unwrap().len();
    assert!(n_after > n_before, "{n_before} -> {n_after}");
    assert_eq!(
        before["meta"]["n_extra_onboarding"].as_u64().unwrap() as usize,
        n_before
    );

    // 上位・満足度4以下・初回の遅れは重ならない
    let top = ids_of(after["rows"].as_array().unwrap());
    let low = ids_of(after["extra_low_satisfaction"].as_array().unwrap());
    let onb = ids_of(after["extra_onboarding_late"].as_array().unwrap());
    let mut all: Vec<&String> = top.iter().chain(low.iter()).chain(onb.iter()).collect();
    let n = all.len();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), n, "上位・満足度4以下・初回の遅れに重なりがある");

    // 遅れのある案件は、必ず今日のどこかに出る（落とさない）。追加の行は全部、初回の遅れがある
    let (mut rows, mut meta) = deal_rows(&sh, day);
    annotate_shared(&mut rows, &mut meta, day, Some(&facts));
    let late: Vec<String> = ids_of(
        &rows
            .iter()
            .filter(|r| r["onboarding_late"] == true)
            .cloned()
            .collect::<Vec<_>>(),
    );
    for r in after["extra_onboarding_late"].as_array().unwrap() {
        assert_eq!(r["onboarding_late"], true);
    }
    let cand = ids_of(after["candidates"].as_array().unwrap());
    for id in &late {
        assert!(
            top.contains(id) || low.contains(id) || onb.contains(id),
            "初回の遅れの案件 {id} が今日に出ていない"
        );
        assert!(cand.contains(id), "担当を選んだときの候補に無い {id}");
    }
    assert_eq!(
        after["meta"]["n_onboarding_only"].as_u64().unwrap() as usize,
        cand.len() - after["meta"]["n_hit"].as_u64().unwrap() as usize
    );
    assert_eq!(
        after["meta"]["n_extra_onboarding"].as_u64().unwrap() as usize,
        onb.len()
    );
    assert_eq!(
        after["meta"]["onboarding"]["n_late_any"].as_u64().unwrap() as usize,
        late.len()
    );
    // 実データの fixture（基準日 2026-09-18）と、上の合成データでの件数
    assert_eq!(
        (late.len(), top.len(), low.len(), onb.len(), cand.len()),
        (111, 24, 32, 95, 295)
    );
    assert_eq!(after["meta"]["n_onboarding_only"], 44);
    eprintln!(
        "TODAY_ONB late={} top={} low={} extra_onb={} onb_only_cand={} cand={}",
        late.len(),
        top.len(),
        low.len(),
        onb.len(),
        after["meta"]["n_onboarding_only"],
        cand.len()
    );

    // 注意の数は初回の印の有無で変わらない
    let flags_of = |v: &Value| -> Vec<(String, u64)> {
        v["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["deal_id"].as_str().unwrap().to_string(),
                    r["n_flags"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    let b = flags_of(&before);
    for (id, nf) in flags_of(&after) {
        if let Some((_, x)) = b.iter().find(|(i, _)| *i == id) {
            assert_eq!(*x, nf);
        }
    }
}
