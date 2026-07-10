//! ゴールデンケーステスト (計画書 §20.3)
//!
//! 代表ケースを固定し、シグナル・矛盾・仮説の出力を構造的に検証する。
//! 実装ケース (面談前=市場側データで再現可能なもの):
//!   1. 給与劣位・市場逼迫
//!   2. 給与優位・応募不足 (市場側観測: 給与上位×長期掲載)
//!   6. 求人多・従業員減
//!   7. サンプル不足
//!  10. 欠損多数
//!
//! ケース3,4 (応募多・面接少 / 面接多・承諾少) はヒアリングデータ依存のため
//! フェーズC/Dの領域であり、本フェーズでは対象外。

use super::evidence_pack::{analyze, to_evidence_pack_json, validate_evidence_references};
use super::hypotheses::HypothesisCategory;
use super::input::{ClientInput, CompanyObservation, ConsultInput};

fn signal_fired(analysis: &super::evidence_pack::ConsultAnalysis, id: &str) -> bool {
    analysis
        .signals
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.fired)
        .unwrap_or(false)
}

/// 全ケース共通の不変条件 (§19 / §24)
fn assert_invariants(analysis: &super::evidence_pack::ConsultAnalysis) {
    // 参照整合: 全仮説の根拠IDが evidence に実在
    let violations = validate_evidence_references(analysis);
    assert!(violations.is_empty(), "参照整合違反: {:?}", violations);

    // 禁止表現 (§19.2) が構造化出力に含まれない
    let json = serde_json::to_string(&to_evidence_pack_json(analysis)).unwrap();
    for banned in [
        "必ず採用できる",
        "応募が増える",
        "離職率が高い企業",
        "成長企業である",
        "この媒体が最適",
        "SalesNow",
    ] {
        assert!(!json.contains(banned), "禁止表現: {}", banned);
    }

    // 全仮説が可能性表現かつ unverified
    for h in &analysis.hypotheses {
        assert!(
            h.statement.contains("可能性"),
            "断定表現の疑い: {}",
            h.statement
        );
        assert_eq!(h.status, "unverified");
    }

    // TOP5上限
    assert!(analysis.top_hypotheses.len() <= 5);
    // 矛盾は config の上限以内 (2026-07-10 強化で最大10)
    assert!(analysis.contradictions.len() <= super::config::CONTRADICTION_MAX);
}

/// ケース1: 給与劣位・市場逼迫
/// 提示給与が市場下位25% + 有効求人倍率高 + 働き手減少
#[test]
fn golden_case_1_salary_disadvantage_tight_market() {
    let input = ConsultInput {
        pref: "大分県".to_string(),
        muni: "大分市".to_string(),
        as_of: "2026-07-10".to_string(),
        data_sources: vec!["今回の求人CSV集計".to_string()],
        total_postings: 160,
        new_count: 15,
        salary_values: (0..100).map(|i| 220_000 + i * 1_500).collect(),
        salary_median: Some(295_000),
        salary_q1: Some(255_000),
        salary_q3: Some(330_000),
        salary_n: 100,
        company_count: 50,
        job_openings_ratio: Some(1.9),
        wa_decline_rate_muni: Some(-17.0),
        scheduled_earnings_latest: Some(290_000.0),
        min_wage_monthly_160h: Some(154_000.0),
        client: ClientInput {
            target_salary_max: Some(230_000), // 分布下位
            hiring_count: Some(2),
            ..Default::default()
        },
        ..Default::default()
    };
    let analysis = analyze(&input);
    assert_invariants(&analysis);

    assert!(signal_fired(&analysis, "S-02"), "提示給与下位25%が発火");
    assert!(signal_fired(&analysis, "S-10"), "求人倍率高が発火");
    assert!(signal_fired(&analysis, "S-07"), "働き手減少が発火");

    // 採用条件仮説が高優先度でTOP5に入る
    assert!(
        analysis
            .top_hypotheses
            .iter()
            .any(|h| h.category == HypothesisCategory::Conditions),
        "給与劣位ケースでは採用条件カテゴリの仮説が上位に来る"
    );
    // 採用目標設計の仮説も生成される (hiring_count入力 + 逼迫市場)
    assert!(
        analysis
            .hypotheses
            .iter()
            .any(|h| h.category == HypothesisCategory::GoalDesign),
        "採用目標設計の仮説が生成される"
    );
}

/// ケース2: 給与優位・応募不足
/// 市場側観測: 提示給与上位25% × 長期掲載比率高 → 矛盾C (給与優位×継続掲載)
#[test]
fn golden_case_2_salary_advantage_low_applications() {
    let input = ConsultInput {
        pref: "群馬県".to_string(),
        muni: "高崎市".to_string(),
        as_of: "2026-07-10".to_string(),
        data_sources: vec!["今回の求人CSV集計".to_string()],
        total_postings: 120,
        new_count: 8,
        salary_values: (0..100).map(|i| 200_000 + i * 1_000).collect(),
        salary_median: Some(250_000),
        salary_q1: Some(225_000),
        salary_q3: Some(275_000),
        salary_n: 100,
        company_count: 35,
        posting_age_30plus_ratio: Some(0.6),
        job_openings_ratio: Some(1.2),
        client: ClientInput {
            target_salary_max: Some(298_000), // 分布上位
            ..Default::default()
        },
        ..Default::default()
    };
    let analysis = analyze(&input);
    assert_invariants(&analysis);

    assert!(signal_fired(&analysis, "S-03"), "提示給与上位25%が発火");
    assert!(signal_fired(&analysis, "S-01"), "長期掲載比率高が発火");

    // 給与優位×継続掲載の矛盾が検出される
    assert!(
        analysis
            .contradictions
            .iter()
            .any(|c| c.title.contains("給与は市場上位")),
        "給与優位×長期掲載の矛盾が検出される: {:?}",
        analysis
            .contradictions
            .iter()
            .map(|c| &c.title)
            .collect::<Vec<_>>()
    );
    // 求人訴求カテゴリの仮説が上位に来る
    assert!(
        analysis
            .top_hypotheses
            .iter()
            .any(|h| h.category == HypothesisCategory::Appeal),
        "求人訴求カテゴリの仮説が上位に来る"
    );
}

/// ケース6: 求人多・従業員減
/// 名寄せ企業の人員減少×募集継続 → S-06 + 矛盾 (欠員補充の可能性)
#[test]
fn golden_case_6_many_postings_employee_decline() {
    let input = ConsultInput {
        pref: "愛知県".to_string(),
        muni: "豊田市".to_string(),
        as_of: "2026-07-10".to_string(),
        data_sources: vec![
            "今回の求人CSV集計".to_string(),
            "企業データベース".to_string(),
        ],
        total_postings: 220,
        new_count: 30,
        salary_values: (0..150).map(|i| 230_000 + i * 800).collect(),
        salary_median: Some(290_000),
        salary_q1: Some(260_000),
        salary_q3: Some(320_000),
        salary_n: 150,
        company_count: 60,
        companies: vec![
            CompanyObservation {
                name: "サンプル部品工業".to_string(),
                posting_count: 15,
                employee_count: Some(800),
                employee_delta_1y: Some(-4.2),
            },
            CompanyObservation {
                name: "サンプル機械".to_string(),
                posting_count: 6,
                employee_count: Some(300),
                employee_delta_1y: Some(2.0),
            },
        ],
        ..Default::default()
    };
    let analysis = analyze(&input);
    assert_invariants(&analysis);

    assert!(signal_fired(&analysis, "S-06"), "従業員減×募集継続が発火");
    assert!(
        analysis
            .contradictions
            .iter()
            .any(|c| c.title.contains("人員減少中")),
        "求人多×従業員減の矛盾が検出される"
    );
    // 定着・離職カテゴリの仮説が生成される
    assert!(
        analysis
            .hypotheses
            .iter()
            .any(|h| h.category == HypothesisCategory::Retention),
        "定着・離職カテゴリの仮説が生成される"
    );
    // 代替説明 (M&A等) が保持されている (§9.1)
    let s06 = analysis.signals.iter().find(|s| s.id == "S-06").unwrap();
    assert!(s06
        .alternative_explanations
        .iter()
        .any(|a| a.contains("組織改編") || a.contains("M&A")));
}

/// ケース7: サンプル不足
/// CSV件数が閾値未満 → S-14 発火 + 強い表現をしない
#[test]
fn golden_case_7_insufficient_sample() {
    let input = ConsultInput {
        pref: "鳥取県".to_string(),
        muni: "米子市".to_string(),
        as_of: "2026-07-10".to_string(),
        data_sources: vec!["今回の求人CSV集計".to_string()],
        total_postings: 8,
        new_count: 2,
        salary_values: vec![210_000, 230_000, 240_000, 255_000, 270_000],
        salary_median: Some(240_000),
        salary_q1: Some(230_000),
        salary_q3: Some(255_000),
        salary_n: 5,
        company_count: 6,
        job_openings_ratio: Some(1.55),
        client: ClientInput {
            target_salary_max: Some(215_000),
            ..Default::default()
        },
        ..Default::default()
    };
    let analysis = analyze(&input);
    assert_invariants(&analysis);

    assert!(signal_fired(&analysis, "S-14"), "サンプル不足が発火");

    // サンプル不足時は仮説の信頼度が High にならない (§11.3 / §19.1)
    for h in &analysis.hypotheses {
        assert_ne!(
            h.confidence,
            super::hypotheses::Confidence::High,
            "サンプル不足時に High 信頼度は出さない: {}",
            h.hypothesis_id
        );
    }
    // S-14 の解釈が弱い表現 (参考程度) を含む
    let s14 = analysis.signals.iter().find(|s| s.id == "S-14").unwrap();
    assert!(s14.interpretation.contains("参考程度"));
}

/// ケース10: 欠損多数
/// 公的統計が全て欠損 → 軸判定は「判定材料不足」、シグナルは data_note で明示、
/// パニックせず出力が生成される
#[test]
fn golden_case_10_mostly_missing_data() {
    let input = ConsultInput {
        pref: "北海道".to_string(),
        // muni 不明・公的統計なし・給与パースもゼロ
        as_of: "2026-07-10".to_string(),
        data_sources: vec!["今回の求人CSV集計".to_string()],
        total_postings: 40,
        new_count: 0,
        company_count: 12,
        ..Default::default()
    };
    let analysis = analyze(&input);
    assert_invariants(&analysis);

    use super::axes::AxisLevel;
    assert_eq!(analysis.axes.demand.level, AxisLevel::Unknown);
    assert_eq!(analysis.axes.supply.level, AxisLevel::Unknown);
    assert_eq!(
        analysis.axes.offer_competitiveness.level,
        AxisLevel::Unknown
    );

    // 判定不能シグナルは fired=false かつ data_note が付く
    let not_evaluable: Vec<_> = analysis
        .signals
        .iter()
        .filter(|s| !s.data_note.is_empty())
        .collect();
    assert!(
        not_evaluable.len() >= 8,
        "欠損多数時は多くのシグナルが判定不能として明示される (実際: {})",
        not_evaluable.len()
    );
    for s in &not_evaluable {
        assert!(!s.fired, "{}: 判定不能なのに発火している", s.id);
    }

    // HTMLも生成できる (パニックしない)
    let html = super::brief_html::render_consult_brief_html(&analysis);
    assert!(html.contains("判定材料不足"));
}
