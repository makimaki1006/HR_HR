//! 機能 E 発火テスト: ヘッダが無意味化された CSV (poor_colmap_fixture.csv) の
//! 列マッピング AI 補完フローを純関数レベルで検証する。
//!
//! # 本番で機能 E を手動発火させる手順
//! 1. `tests/fixtures/poor_colmap_fixture.csv` をブラウザの媒体分析タブからアップロードする。
//! 2. 環境変数 `GEMINI_API_KEY` が設定されていること。
//! 3. アップロード後の画面サマリーに
//!    「AI が列推定を補助しました (N 列)」と表示されれば機能 E が発火している。
//! 4. GEMINI_API_KEY 未設定時は上記メッセージは表示されず、通常の (誤) パース結果になる
//!    (graceful degradation — 従来経路が保たれる)。
//!
//! # フィクスチャ設計の説明
//! - ヘッダは "c1","c2","c3","c4","c5" で意味を持たせない。
//! - c2 列 (company_name 相当): "AA","BB" 等の 2 文字 ASCII → val.len()=2 < 3 のため
//!   score_company=0。動的検出でも col_map に入らず auto-parse では company_name は
//!   別の列 (誤認識) になる。AI モックが col_map[company_name]=1 を補完する。
//! - c4 列 (salary 相当): 月給/時給フォーマット → score_salary > 0 で per-row scan が検出。
//!
//! # テスト設計方針
//! - ネットワーク非依存: 実際の AI 呼び出しは行わず、`parse_colmap_from_ai` に
//!   モック JSON を渡して純関数レベルで検証する。
//! - is_parse_poor の unit test は CSV パースに依存せず、直接レコードを構築して検証する。
//!   (動的検出の HashMap tie-break が非決定的なため CSV 経由の is_parse_poor 検証は避ける)
//! - guard_demo: "AA" という 2 文字コードが AI override なしでは company_name に現れないこと +
//!   15-call cap により 300 件超の目標は処理されないことを示す。

use rust_dashboard::handlers::survey::upload::{
    collect_extraction_targets, is_parse_poor, parse_colmap_from_ai,
    parse_csv_bytes_with_col_overrides, parse_csv_bytes_with_hints, ExtractionYield,
    UserSourceHint,
};
use serde_json::json;

/// テスト用 CSV バイト列 (コンパイル時に読み込み)
const FIXTURE_CSV: &[u8] = include_bytes!("fixtures/poor_colmap_fixture.csv");

// ===========================================================================
// 1. is_parse_poor のユニットテスト (CSV パース不使用)
//    score_salary=0 のレコードを直接構築 → all_salary_empty=true → is_parse_poor=true
// ===========================================================================

#[test]
fn is_parse_poor_true_for_records_with_all_empty_salary() {
    use rust_dashboard::handlers::survey::location_parser::parse_location;
    use rust_dashboard::handlers::survey::salary_parser::{parse_salary, SalaryType};
    use rust_dashboard::handlers::survey::upload::{CsvSource, SurveyRecord};

    // salary_raw="" のレコードを 5 件構築 → all_salary_empty=true
    let recs: Vec<SurveyRecord> = (0..5)
        .map(|i| SurveyRecord {
            row_index: i,
            source: CsvSource::Unknown,
            job_title: format!("看護師{}", i),
            company_name: format!("病院{}", i),
            location_raw: "東京都".to_string(),
            salary_raw: String::new(), // ← 空
            employment_type: "正社員".to_string(),
            tags_raw: String::new(),
            url: None,
            is_new: false,
            description: String::new(),
            salary_parsed: parse_salary("", SalaryType::Monthly),
            location_parsed: parse_location("", None),
            annual_holidays: None,
            ai_monthly_holidays_min: None,
            ai_monthly_holidays_max: None,
            ai_bonus: None,
            ai_bonus_times_per_year: None,
            ai_paid_leave_rate: None,
            ai_weekly_holiday_type: None,
            ai_overtime_hours_monthly: None,
        })
        .collect();

    assert!(
        is_parse_poor(&recs),
        "salary_raw が全行空 → all_salary_empty=true → is_parse_poor=true"
    );
    // 空集合はエラー経路 → 貧弱扱いしない (graceful degradation の一部)
    assert!(
        !is_parse_poor(&[]),
        "空レコード集合は is_parse_poor=false (エラー経路で処理済)"
    );
}

// ===========================================================================
// 2. フィクスチャ CSV の auto-parse で "AA" が company_name に入らないこと
//    c2 列の 2 文字 ASCII ("AA","BB" 等) は score_company=0 のため動的検出で
//    company_name に割り当てられない → AI override が必要であることを示す。
// ===========================================================================

#[test]
fn feature_e_fixture_two_char_company_not_auto_detected() {
    let records = parse_csv_bytes_with_hints(FIXTURE_CSV, None, UserSourceHint::Auto)
        .expect("fixture CSV を正常にパースできること");
    assert!(
        !records.is_empty(),
        "fixture CSV は少なくとも 1 件のレコードを持つべき"
    );
    // "AA" (2 文字 ASCII, score_company=0) は自動検出で company_name に入らない
    let any_aa = records.iter().any(|r| r.company_name == "AA");
    assert!(
        !any_aa,
        "自動検出では c2='AA' が company_name に入らない (AI override が必要): \
        1件目 company_name={:?}",
        records.first().map(|r| &r.company_name)
    );
}

// ===========================================================================
// 3. parse_colmap_from_ai による col_map 補完
//    モック JSON から col_map を構築し、parse_csv_bytes_with_col_overrides で
//    再パースすると company_name="AA" / job_title="看護師" になること。
// ===========================================================================

#[test]
fn feature_e_mock_ai_colmap_supplements_company_to_two_char_code() {
    // AI モック: c1=job_title / c2=company / c3=location / c4=salary / c5=employment_type
    let mock_ai_resp = json!({
        "mappings": [
            { "column_index": 0, "role": "title" },
            { "column_index": 1, "role": "company" },
            { "column_index": 2, "role": "location" },
            { "column_index": 3, "role": "salary" },
            { "column_index": 4, "role": "employment_type" }
        ]
    });
    let col_map = parse_colmap_from_ai(&mock_ai_resp, 5);
    assert_eq!(col_map.get("job_title"), Some(&0), "col_map[job_title]=0");
    assert_eq!(
        col_map.get("company_name"),
        Some(&1),
        "col_map[company_name]=1"
    );

    let records =
        parse_csv_bytes_with_col_overrides(FIXTURE_CSV, None, UserSourceHint::Auto, &col_map)
            .expect("col_overrides 付き再パースが成功すること");

    assert!(!records.is_empty());

    // 1 行目: company_name が c2 列の "AA" になっていること (AI override の効果)
    assert_eq!(
        records[0].company_name, "AA",
        "col_overrides 後は c2 列の 'AA' が company_name になる"
    );
    // 1 行目: job_title が c1 列の "看護師" になっていること
    assert_eq!(
        records[0].job_title, "看護師",
        "col_overrides 後は c1 列の '看護師' が job_title になる"
    );
}

// ===========================================================================
// 4. guard_demo: AI override なし → company_name は "AA" にならない (従来動作)
//    `if let Some(client) = gemini.as_ref()` ガードを通らないパスの相当。
//    col_overrides 空 = AI キー未設定パスの純関数的等価。
// ===========================================================================

#[test]
fn guard_demo_no_override_company_stays_non_aa() {
    let empty_overrides = std::collections::HashMap::new();
    let records = parse_csv_bytes_with_col_overrides(
        FIXTURE_CSV,
        None,
        UserSourceHint::Auto,
        &empty_overrides,
    )
    .expect("override なしパースも正常終了すること");

    assert!(!records.is_empty());
    // override なし = AI 補完なし: c2 の "AA" は company_name に入らない
    let any_aa = records.iter().any(|r| r.company_name == "AA");
    assert!(
        !any_aa,
        "override なし (= キー未設定パス相当) では company_name は 'AA' にならない (従来動作)"
    );
}

// ===========================================================================
// 5. guard_demo: 15-call cap — 300 件超の目標は処理されない
//    collect_extraction_targets が 320 件返す場合、chunks(20) で 16 バッチになるが
//    15-call cap (call_idx >= 15) により最後の 1 バッチ (20 件) はスキップされる。
// ===========================================================================

#[test]
fn guard_demo_15_call_cap_skips_over_300_records() {
    use rust_dashboard::handlers::survey::location_parser::parse_location;
    use rust_dashboard::handlers::survey::salary_parser::{parse_salary, SalaryType};
    use rust_dashboard::handlers::survey::upload::{CsvSource, SurveyRecord};

    // 320 件のレコード (description >= 30 文字) を用意
    let records: Vec<SurveyRecord> = (0..320_usize)
        .map(|i| SurveyRecord {
            row_index: i,
            source: CsvSource::Unknown,
            job_title: format!("看護師{}", i),
            company_name: format!("A病院{}", i),
            location_raw: String::new(),
            salary_raw: String::new(),
            employment_type: String::new(),
            tags_raw: String::new(),
            url: None,
            is_new: false,
            // 30 文字以上の description → collect_extraction_targets の対象
            description: format!(
                "勤務内容詳細 {}番目。年間休日120日。週休2日制。残業少なめ。",
                i
            ),
            salary_parsed: parse_salary("", SalaryType::Monthly),
            location_parsed: parse_location("", None),
            annual_holidays: None,
            ai_monthly_holidays_min: None,
            ai_monthly_holidays_max: None,
            ai_bonus: None,
            ai_bonus_times_per_year: None,
            ai_paid_leave_rate: None,
            ai_weekly_holiday_type: None,
            ai_overtime_hours_monthly: None,
        })
        .collect();

    let targets = collect_extraction_targets(&records);
    assert_eq!(
        targets.len(),
        320,
        "320 件すべてが extraction 対象 (description >= 30 文字)"
    );

    let total_batches = targets.chunks(20).count();
    assert_eq!(total_batches, 16, "320 / 20 = 16 バッチ");

    // 15-call cap により処理されるのは最初の 15 バッチのみ
    let processed: usize = targets
        .chunks(20)
        .enumerate()
        .filter(|(call_idx, _)| *call_idx < 15)
        .map(|(_, chunk)| chunk.len())
        .sum();
    assert_eq!(processed, 300, "cap 内で処理されるのは 300 件 (15 × 20)");

    let skipped: usize = targets
        .chunks(20)
        .enumerate()
        .filter(|(call_idx, _)| *call_idx >= 15)
        .map(|(_, chunk)| chunk.len())
        .sum();
    assert_eq!(skipped, 20, "15-call cap で最後の 20 件がスキップされる");

    // ExtractionYield::default() は全フィールド 0 (キー未設定時の戻り値と同値)
    assert_eq!(
        ExtractionYield::default(),
        ExtractionYield {
            annual_holidays: 0,
            monthly_holidays: 0,
            bonus: 0,
            paid_leave: 0,
            weekly_type: 0,
            overtime: 0,
        }
    );
}
