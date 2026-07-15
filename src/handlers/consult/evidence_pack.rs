//! evidence_pack.json 出力 (計画書 §15.2) と分析パイプライン
//!
//! 分析の実行順序 (§4):
//!   入力スナップショット → 証拠登録 → 4軸指標 → シグナル → 矛盾 → 仮説 → 質問
//!
//! 出力は §15.2 の形式:
//!   { report_meta, evidence, metrics, signals, contradictions, hypotheses, questions, actions }
//!
//! actions はフェーズD (ヒアリング後) の領域のため常に空配列。
//! 文章生成 (ブリーフHTML) はこの構造化結果だけを参照する (§15.2「原データへ直接
//! アクセスして自由解釈させない」)。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::axes::{judge_axes, AxesJudgment};
use super::contradictions::{detect_contradictions, Contradiction};
use super::evidence::{Evidence, EvidenceStore};
use super::hypotheses::{build_hypotheses, top_hypotheses, Hypothesis};
use super::input::ConsultInput;
use super::questions::{generate_questions, Question};
use super::signals::{evaluate_signals, Signal};

/// レポートメタ情報 (§15.2 report_meta)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMeta {
    /// 生成日 (データ基準日)
    pub generated_at: String,
    /// 対象都道府県
    pub prefecture: String,
    /// 対象市区町村
    pub municipality: String,
    /// 職種メモ
    pub occupation_note: String,
    /// 利用データ一覧
    pub data_sources: Vec<String>,
    /// 顧客任意入力の要約 (入力があった項目のみ)
    pub client_input_summary: Vec<String>,
    /// 取り扱い区分 (常に社内用)
    pub classification: String,
}

/// 分析結果一式 (ブリーフHTML描画とJSON出力の共通入力)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsultAnalysis {
    pub report_meta: ReportMeta,
    pub evidence: Vec<Evidence>,
    /// 4軸判定 (§8。総合点は存在しない)
    pub axes: AxesJudgment,
    pub signals: Vec<Signal>,
    pub contradictions: Vec<Contradiction>,
    /// 生成された全仮説 (priority×confidence 降順)
    pub hypotheses: Vec<Hypothesis>,
    /// TOP5 仮説
    pub top_hypotheses: Vec<Hypothesis>,
    pub questions: Vec<Question>,
}

/// 分析パイプライン本体。同一入力から同一の構造化結果を再生成できる (§24-1 決定的)。
pub fn analyze(input: &ConsultInput) -> ConsultAnalysis {
    let mut store = EvidenceStore::new();

    let axes = judge_axes(input, &mut store);
    let signals = evaluate_signals(input, &mut store);
    let contradictions = detect_contradictions(&signals);
    let hypotheses = build_hypotheses(input, &signals, &store);
    let top = top_hypotheses(&hypotheses);
    let questions = generate_questions(&top);

    // 全証拠が確定した後、現象テーマを決定的に付与する (mint 箇所のエンジン側処理)。
    // evidence ID は動的採番のため、テーマは metric_name / granularity から classify する。
    let mut evidence = store.items().to_vec();
    super::theme::assign_themes(&mut evidence);

    let mut client_summary = Vec::new();
    if let (Some(min), Some(max)) = (
        input.client.target_salary_min,
        input.client.target_salary_max,
    ) {
        client_summary.push(format!("提示給与: {}〜{}円", min, max));
    } else if let Some(v) = input
        .client
        .target_salary_min
        .or(input.client.target_salary_max)
    {
        client_summary.push(format!("提示給与: {}円", v));
    }
    if let Some(n) = input.client.hiring_count {
        client_summary.push(format!("採用予定人数: {}名", n));
    }
    if let Some(d) = input.client.deadline.as_ref().filter(|s| !s.is_empty()) {
        client_summary.push(format!("採用期限: {}", d));
    }
    if let Some(note) = input.client.note.as_ref().filter(|s| !s.is_empty()) {
        client_summary.push(format!("メモ: {}", note));
    }

    let report_meta = ReportMeta {
        generated_at: input.as_of.clone(),
        prefecture: input.pref.clone(),
        municipality: input.muni.clone(),
        occupation_note: input.occupation_note.clone(),
        data_sources: input.data_sources.clone(),
        client_input_summary: client_summary,
        classification: "社内用 — 顧客配布不可".to_string(),
    };

    ConsultAnalysis {
        report_meta,
        evidence,
        axes,
        signals,
        contradictions,
        hypotheses,
        top_hypotheses: top,
        questions,
    }
}

/// §15.2 形式の evidence_pack JSON を生成する。
/// actions はフェーズD (ヒアリング後) の領域のため空配列。
pub fn to_evidence_pack_json(analysis: &ConsultAnalysis) -> Value {
    serde_json::json!({
        "report_meta": analysis.report_meta,
        "evidence": analysis.evidence,
        "metrics": {
            "axes": analysis.axes,
        },
        "signals": analysis.signals,
        "contradictions": analysis.contradictions,
        "hypotheses": analysis.hypotheses,
        "questions": analysis.questions,
        "actions": [],
    })
}

/// 参照整合の検証 (§19.1「仮説に根拠IDが存在するか」):
/// 全仮説・シグナル・矛盾・軸判定が参照する証拠IDが evidence に実在することを確認。
/// 不整合があれば違反メッセージの一覧を返す (空=整合)。
pub fn validate_evidence_references(analysis: &ConsultAnalysis) -> Vec<String> {
    let mut violations = Vec::new();
    let exists = |id: &str| -> bool { analysis.evidence.iter().any(|e| e.id == id) };

    for axis in analysis.axes.all() {
        for id in &axis.evidence_ids {
            if !exists(id) {
                violations.push(format!("axis {}: 証拠ID {} が実在しない", axis.axis, id));
            }
        }
    }
    for s in &analysis.signals {
        for id in &s.evidence_ids {
            if !exists(id) {
                violations.push(format!("signal {}: 証拠ID {} が実在しない", s.id, id));
            }
        }
    }
    for c in &analysis.contradictions {
        for id in &c.evidence_ids {
            if !exists(id) {
                violations.push(format!(
                    "contradiction {}: 証拠ID {} が実在しない",
                    c.contradiction_id, id
                ));
            }
        }
    }
    for h in &analysis.hypotheses {
        if h.supporting_evidence_ids.is_empty() {
            violations.push(format!("hypothesis {}: 根拠IDがない", h.hypothesis_id));
        }
        for id in h
            .supporting_evidence_ids
            .iter()
            .chain(h.counter_evidence_ids.iter())
        {
            if !exists(id) {
                violations.push(format!(
                    "hypothesis {}: 証拠ID {} が実在しない",
                    h.hypothesis_id, id
                ));
            }
        }
    }
    for q in &analysis.questions {
        if !analysis
            .hypotheses
            .iter()
            .any(|h| h.hypothesis_id == q.related_hypothesis_id)
        {
            violations.push(format!(
                "question {}: 関連仮説 {} が実在しない",
                q.question_id, q.related_hypothesis_id
            ));
        }
    }
    violations
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::handlers::consult::input::{ClientInput, CompanyObservation};

    /// 標準的な合成入力 (多数のシグナルが発火する)
    pub(crate) fn rich_input() -> ConsultInput {
        ConsultInput {
            pref: "群馬県".to_string(),
            muni: "高崎市".to_string(),
            occupation_note: "介護・福祉系を除く一般事務".to_string(),
            as_of: "2026-07-10".to_string(),
            data_sources: vec![
                "今回の求人CSV集計".to_string(),
                "毎月勤労統計 地方調査".to_string(),
                "地域別最低賃金".to_string(),
                "就業構造基本調査".to_string(),
                "国立社会保障・人口問題研究所 将来人口推計".to_string(),
                "国勢調査 通勤・通学OD".to_string(),
                "企業データベース".to_string(),
            ],
            total_postings: 180,
            new_count: 20,
            is_hourly: false,
            salary_values: (0..120).map(|i| 195_000 + i * 1_200).collect(),
            salary_median: Some(265_000),
            salary_q1: Some(230_000),
            salary_q3: Some(295_000),
            salary_n: 120,
            hourly_median_low: None,
            posting_age_30plus_ratio: Some(0.55),
            company_count: 45,
            companies: vec![
                CompanyObservation {
                    name: "サンプル運輸".to_string(),
                    posting_count: 12,
                    employee_count: Some(320),
                    employee_delta_1y: Some(-6.5),
                },
                CompanyObservation {
                    name: "サンプル製作所".to_string(),
                    posting_count: 8,
                    employee_count: Some(150),
                    employee_delta_1y: Some(3.0),
                },
            ],
            scheduled_earnings_latest: Some(310_000.0),
            min_wage_hourly: Some(985.0),
            min_wage_monthly_160h: Some(157_600.0),
            job_openings_ratio: Some(1.62),
            job_change_desire_rate_pref: Some(8.2),
            job_change_desire_rate_national: Some(10.5),
            wa_decline_rate_muni: Some(-18.4),
            commute_inflow_total: Some(24_500),
            commute_outflow_total: Some(15_200),
            commute_inflow_top3: vec![
                ("群馬県".to_string(), "前橋市".to_string(), 8_200),
                ("群馬県".to_string(), "藤岡市".to_string(), 4_100),
                ("埼玉県".to_string(), "本庄市".to_string(), 2_900),
            ],
            // 拡充データ (公的統計)
            net_migration_rate: Some(-3.2),
            daytime_ratio: Some(94.5),
            // 経済センサス調査間 累計 (調査間隔5年)。年換算 開4.2/廃5.6 → 廃業超過が発火
            business_opening_rate: Some(21.0),
            business_closure_rate: Some(28.0),
            business_dynamics_interval_years: Some(5.0),
            unemployment_rate_pref: Some(2.1),
            unemployment_rate_national: Some(2.8),
            natural_change: Some(-1_800),
            // 1畳あたり家賃 (総数)。県2200円 / 全国1200円 ≈ 1.83倍 → 相対的に高いで発火
            rent_per_tatami: Some(2_200),
            rent_per_tatami_national: Some(1_200),
            // 拡充データ (媒体CSV観測)
            distinct_tag_count: 4,
            top_tags: vec![
                ("交通費支給".to_string(), 90),
                ("賞与あり".to_string(), 60),
                ("週休二日".to_string(), 40),
            ],
            popular_ratio: Some(0.31),
            super_popular_count: 6,
            annual_holidays_median: Some(110),
            annual_holidays_n: 42,
            holiday_pct_ge_120: Some(0.18),
            employment_type_dist: vec![
                ("パート・アルバイト".to_string(), 100),
                ("正社員".to_string(), 60),
                ("契約社員".to_string(), 20),
            ],
            muni_dist_top: vec![("高崎市".to_string(), 110), ("前橋市".to_string(), 40)],
            client: ClientInput {
                target_salary_min: Some(300_000),
                target_salary_max: Some(320_000),
                hiring_count: Some(3),
                deadline: Some("2026年9月末".to_string()),
                note: Some("夜勤なし希望".to_string()),
            },
        }
    }

    #[test]
    fn analyze_is_deterministic() {
        let input = rich_input();
        let a1 = analyze(&input);
        let a2 = analyze(&input);
        let j1 = serde_json::to_string(&to_evidence_pack_json(&a1)).unwrap();
        let j2 = serde_json::to_string(&to_evidence_pack_json(&a2)).unwrap();
        assert_eq!(j1, j2, "同一入力から同一の構造化結果を再生成できる (§24-1)");
    }

    #[test]
    fn evidence_references_are_consistent() {
        let analysis = analyze(&rich_input());
        let violations = validate_evidence_references(&analysis);
        assert!(violations.is_empty(), "参照整合違反: {:?}", violations);
    }

    #[test]
    fn pack_json_has_spec_shape() {
        let analysis = analyze(&rich_input());
        let pack = to_evidence_pack_json(&analysis);
        // §15.2 のトップレベルキーが全て存在
        for key in [
            "report_meta",
            "evidence",
            "metrics",
            "signals",
            "contradictions",
            "hypotheses",
            "questions",
            "actions",
        ] {
            assert!(pack.get(key).is_some(), "キー {} がない", key);
        }
        // actions はフェーズDまで空
        assert_eq!(pack["actions"].as_array().unwrap().len(), 0);
        // classification が社内用
        assert_eq!(
            pack["report_meta"]["classification"].as_str().unwrap(),
            "社内用 — 顧客配布不可"
        );
    }

    #[test]
    fn top5_never_exceeds_five() {
        let analysis = analyze(&rich_input());
        assert!(analysis.top_hypotheses.len() <= 5);
        assert!(
            !analysis.top_hypotheses.is_empty(),
            "多数のシグナル発火時は仮説が生成される"
        );
    }

    #[test]
    fn pack_json_does_not_contain_internal_names() {
        // 内部テーブル名・外部サービス名を出力に含めない
        let analysis = analyze(&rich_input());
        let json = serde_json::to_string(&to_evidence_pack_json(&analysis)).unwrap();
        for banned in [
            "SalesNow",
            "salesnow",
            "cross_future_workforce",
            "cross_wage_public",
            "cross_switcher_supply",
            "v2_external",
            "v2_salesnow",
            "ts_turso",
            "hw_",
        ] {
            assert!(
                !json.contains(banned),
                "出力に内部名/サービス名 {} が含まれている",
                banned
            );
        }
    }
}
