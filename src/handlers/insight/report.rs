//! レポートJSON生成（4章ストーリー構成）

use super::super::helpers::{format_number, get_f64, get_str_ref};
use super::fetch::InsightContext;
use super::helpers::*;
use serde_json::{json, Value};

/// 全国平均欠員率（HWデータ全体統計より）
pub(crate) const NATIONAL_AVG_VACANCY_RATE: f64 = 0.266;

/// レポートJSON構築
pub fn build_report_json(insights: &[Insight], pref: &str, muni: &str) -> Value {
    let location = if !muni.is_empty() {
        format!("{} {}", pref, muni)
    } else if !pref.is_empty() {
        pref.to_string()
    } else {
        "全国".to_string()
    };

    let executive_summary = generate_executive_summary_text_simple(insights);

    let chapters = vec![
        build_chapter(
            1,
            "現状把握 — この地域の求人市場は今どうなっているか",
            insights,
            InsightCategory::HiringStructure,
            "求人市場の構造的な特徴と課題を分析します。",
        ),
        build_chapter(
            2,
            "トレンド分析 — 市場はどこに向かっているか",
            insights,
            InsightCategory::Forecast,
            "時系列データと人口動態から、今後の市場動向を予測します。",
        ),
        build_chapter(
            3,
            "地域ポジショニング — 他地域と比べてどうか",
            insights,
            InsightCategory::RegionalCompare,
            "同じ都道府県内の他地域との比較から、相対的な位置づけを明らかにします。",
        ),
        build_chapter(
            4,
            "推奨アクション — 何をすべきか",
            insights,
            InsightCategory::ActionProposal,
            "分析結果に基づく具体的な改善施策を、優先度順に提案します。",
        ),
    ];

    json!({
        "title": "ハローワーク求人市場 総合診断レポート",
        "subtitle": format!("{} | {}", location, chrono::Local::now().format("%Y年%m月")),
        "location": location,
        "generated_at": chrono::Local::now().to_rfc3339(),
        "executive_summary": executive_summary,
        "insight_counts": {
            "critical": insights.iter().filter(|i| i.severity == Severity::Critical).count(),
            "warning": insights.iter().filter(|i| i.severity == Severity::Warning).count(),
            "info": insights.iter().filter(|i| i.severity == Severity::Info).count(),
            "positive": insights.iter().filter(|i| i.severity == Severity::Positive).count(),
        },
        "chapters": chapters,
    })
}

/// エグゼクティブサマリー生成（具体的な数値を含む）
pub(crate) fn generate_executive_summary_text(
    insights: &[Insight],
    ctx: &InsightContext,
) -> String {
    let critical = insights
        .iter()
        .filter(|i| i.severity == Severity::Critical)
        .count();
    let warning = insights
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let positive = insights
        .iter()
        .filter(|i| i.severity == Severity::Positive)
        .count();
    let total = insights.len();

    // 欠員率（正社員）
    let vacancy_rate = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .unwrap_or(0.0);

    // 平均月給（正社員）
    let avg_salary = ctx
        .cascade
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "avg_salary_min") as i64)
        .unwrap_or(0);

    // 全国平均との比較で判定
    let overall = if critical >= 3 && vacancy_rate > 0.30 {
        "深刻な課題を抱えています"
    } else if critical >= 1 {
        "いくつかの重要な課題があります"
    } else if warning >= 3 {
        "改善の余地があります"
    } else if positive >= 2 {
        "比較的良好な状態です"
    } else {
        "標準的な状態です"
    };

    let mut summary = format!("この地域の求人市場は{}。", overall);

    // 具体的なKPIを記載
    let vr_pct = vacancy_rate * 100.0;
    let national_pct = NATIONAL_AVG_VACANCY_RATE * 100.0;
    let vr_compare = if vacancy_rate > NATIONAL_AVG_VACANCY_RATE {
        format!("全国平均{:.1}%を上回る", national_pct)
    } else {
        format!("全国平均{:.1}%を下回る", national_pct)
    };
    summary.push_str(&format!("欠員率{:.1}%（{}）", vr_pct, vr_compare));

    if avg_salary > 0 {
        summary.push_str(&format!("、正社員平均月給{}円", format_number(avg_salary)));
    }

    if ctx.commute_zone_total_pop > 0 {
        let pop_man = ctx.commute_zone_total_pop as f64 / 10_000.0;
        summary.push_str(&format!("、通勤圏人口{:.1}万人", pop_man));
    }

    summary.push_str(&format!(
        "。全{}件の分析指標のうち、重大{}件・注意{}件・良好{}件。",
        total, critical, warning, positive
    ));

    // 最優先アクションを明示
    if let Some(top_action) = insights
        .iter()
        .find(|i| i.category == InsightCategory::ActionProposal)
    {
        summary.push_str(&format!("最優先アクション: {}。", top_action.title));
    } else if let Some(top) = insights.first() {
        summary.push_str(&format!("最優先課題: {}。", top.title));
    }

    summary
}

/// 旧シグネチャ互換（report JSON用）
pub(crate) fn generate_executive_summary_text_simple(insights: &[Insight]) -> String {
    let critical = insights
        .iter()
        .filter(|i| i.severity == Severity::Critical)
        .count();
    let warning = insights
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let positive = insights
        .iter()
        .filter(|i| i.severity == Severity::Positive)
        .count();
    let total = insights.len();
    let overall = if critical >= 3 {
        "深刻な課題を抱えています"
    } else if critical >= 1 {
        "いくつかの重要な課題があります"
    } else if warning >= 3 {
        "改善の余地があります"
    } else if positive >= 2 {
        "比較的良好な状態です"
    } else {
        "標準的な状態です"
    };
    let mut s = format!(
        "この地域の求人市場は{}。全{}件中、重大{}件・注意{}件・良好{}件。",
        overall, total, critical, warning, positive
    );
    if let Some(top) = insights.first() {
        s.push_str(&format!("最優先課題: {}。", top.title));
    }
    s
}

/// 章の構築
fn build_chapter(
    number: u8,
    title: &str,
    all_insights: &[Insight],
    category: InsightCategory,
    intro: &str,
) -> Value {
    let chapter_insights: Vec<&Insight> = all_insights
        .iter()
        .filter(|i| i.category == category)
        .collect();

    let narrative = if chapter_insights.is_empty() {
        format!("{}現時点では特筆すべき事項は検出されませんでした。", intro)
    } else {
        let mut text = intro.to_string();
        for (i, insight) in chapter_insights.iter().enumerate() {
            if i == 0 {
                text.push_str(&format!(
                    "\n\n最も重要な点として、{}。{}",
                    insight.title, insight.body
                ));
            } else {
                text.push_str(&format!("\n\nまた、{}。{}", insight.title, insight.body));
            }
        }
        text
    };

    let insights_json: Vec<Value> = chapter_insights
        .iter()
        .map(|i| {
            json!({
                "id": i.id,
                "severity": i.severity.label(),
                "title": i.title,
                "body": i.body,
                "evidence": i.evidence.iter().map(|e| json!({
                    "metric": e.metric,
                    "value": e.value,
                    "unit": e.unit,
                    "context": e.context,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({
        "number": number,
        "title": title,
        "narrative": narrative,
        "insight_count": chapter_insights.len(),
        "insights": insights_json,
    })
}

// ======== レポートHTML用 章ナラティブ生成 ========

/// 章ごとのナラティブ（「問い→答え」形式、具体数値入り）
pub(crate) fn generate_chapter_narrative(
    category: &InsightCategory,
    insights: &[&Insight],
    ctx: &InsightContext,
) -> String {
    if insights.is_empty() {
        return "現時点では特筆すべき事項は検出されませんでした。".to_string();
    }
    let critical_count = insights
        .iter()
        .filter(|i| i.severity == Severity::Critical)
        .count();
    let warning_count = insights
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();

    match category {
        InsightCategory::HiringStructure => {
            let vacancy_rate = ctx
                .vacancy
                .iter()
                .find(|r| get_str_ref(r, "emp_group") == "正社員")
                .map(|r| get_f64(r, "vacancy_rate"))
                .unwrap_or(0.0);
            let vr_pct = vacancy_rate * 100.0;
            let vr_judge = if vacancy_rate > 0.30 {
                "深刻な水準"
            } else if vacancy_rate > NATIONAL_AVG_VACANCY_RATE {
                "全国平均以上"
            } else {
                "全国平均以下"
            };

            let mut text = format!(
                "構造的課題は{}件（重大{}件、注意{}件）。正社員欠員率{:.1}%（{}）。",
                insights.len(),
                critical_count,
                warning_count,
                vr_pct,
                vr_judge
            );
            if ctx.commute_self_rate > 0.0 {
                text.push_str(&format!(
                    "地元就業率{:.1}%。",
                    ctx.commute_self_rate * 100.0
                ));
            }
            if let Some(top) = insights.first() {
                text.push_str(&format!("最も深刻な課題は「{}」。", top.title));
            }
            text
        }
        InsightCategory::Forecast => {
            let mut text = format!("{}件のトレンド指標を検出。", insights.len());
            // 高齢化率
            if !ctx.ext_pyramid.is_empty() {
                let mut elderly: i64 = 0;
                let mut total: i64 = 0;
                for row in &ctx.ext_pyramid {
                    let pop = super::super::helpers::get_i64(row, "male_count")
                        + super::super::helpers::get_i64(row, "female_count");
                    total += pop;
                    let age = get_str_ref(row, "age_group");
                    match age {
                        "65-69" | "70-74" | "75-79" | "80-84" | "85+" | "70-79" | "80+" => {
                            elderly += pop
                        }
                        _ => {}
                    }
                }
                if total > 0 {
                    let aging_rate = elderly as f64 / total as f64 * 100.0;
                    let aging_judge = if aging_rate > 30.0 {
                        "深刻"
                    } else if aging_rate > 25.0 {
                        "進行中"
                    } else {
                        "比較的若い"
                    };
                    text.push_str(&format!("高齢化率{:.1}%（{}）。", aging_rate, aging_judge));
                }
            }
            if critical_count > 0 {
                if let Some(top) = insights.iter().find(|i| i.severity == Severity::Critical) {
                    text.push_str(&format!("最も警戒すべきは「{}」。", top.title));
                }
            } else {
                text.push_str("現時点で重大なトレンドリスクは検出されていない。");
            }
            text
        }
        InsightCategory::RegionalCompare => {
            let inferior = insights
                .iter()
                .filter(|i| i.severity == Severity::Critical || i.severity == Severity::Warning)
                .count();
            let mut text = format!(
                "{}件の地域比較指標のうち、{}件が他地域に対して劣位。",
                insights.len(),
                inferior
            );
            if inferior == 0 {
                text.push_str("地域間比較では概ね良好な位置にある。");
            } else if let Some(top) = insights.first() {
                text.push_str(&format!("最も改善が必要な指標は「{}」。", top.title));
            }
            text
        }
        InsightCategory::ActionProposal => {
            let mut text = format!("{}件の改善施策を提案。", insights.len());
            if let Some(top) = insights.first() {
                let cost_hint = match top.id.as_str() {
                    "AP-1" => "コストあり・高インパクト",
                    "AP-2" => "コストゼロ・即日実行可能",
                    "AP-3" => "低コスト・中インパクト",
                    _ => "優先度高",
                };
                text.push_str(&format!("最優先は「{}」（{}）。", top.title, cost_hint));
            }
            text
        }
        InsightCategory::StructuralContext => {
            let mut text = format!(
                "{}件の構造指標を検出（重大{}件、注意{}件）。",
                insights.len(),
                critical_count,
                warning_count
            );
            if let Some(top) = insights.first() {
                text.push_str(&format!("最も着目すべきは「{}」。", top.title));
            }
            if insights.is_empty() {
                text = "構造指標での有意な示唆は検出されていない傾向がみられる。".to_string();
            }
            text
        }
    }
}

// ======== その他ナラティブ生成 ========

/// 採用困難度グレード
pub(crate) struct DifficultyGrade {
    pub letter: &'static str,
    pub label: &'static str,
    pub color: &'static str,
}

/// 採用困難度を算出
pub(crate) fn compute_difficulty_grade(insights: &[Insight], vacancy_rate: f64) -> DifficultyGrade {
    let critical = insights
        .iter()
        .filter(|i| i.severity == Severity::Critical)
        .count();
    let warning = insights
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let positive = insights
        .iter()
        .filter(|i| i.severity == Severity::Positive)
        .count();

    if critical >= 3 && vacancy_rate > 0.30 {
        DifficultyGrade {
            letter: "D",
            label: "深刻",
            color: "#dc2626",
        }
    } else if critical >= 1 || vacancy_rate > 0.25 {
        DifficultyGrade {
            letter: "C",
            label: "困難",
            color: "#f59e0b",
        }
    } else if vacancy_rate > 0.20 || warning >= 3 {
        DifficultyGrade {
            letter: "B",
            label: "やや困難",
            color: "#eab308",
        }
    } else if positive > warning {
        DifficultyGrade {
            letter: "A",
            label: "良好",
            color: "#059669",
        }
    } else {
        DifficultyGrade {
            letter: "B-",
            label: "標準",
            color: "#6b7280",
        }
    }
}

/// insightの「つまり」テキスト（So What?）
pub(crate) fn generate_so_what(insight: &Insight) -> String {
    match insight.id.as_str() {
        "HS-1" => "→ 欠員が慢性化しており、現行条件での充足は困難。給与・勤務条件の見直しか採用チャネル拡大が必要".into(),
        "HS-2" => "→ 給与水準が市場を下回っている。中央値への引き上げで応募数改善が見込める".into(),
        "HS-3" => "→ 求人情報の開示項目を増やすだけで応募率が改善する（コストゼロで実行可能）".into(),
        "HS-4" => "→ 求人原稿が実態の逼迫度を反映していない。急募キーワードや待遇の具体記載を追加すべき".into(),
        "HS-5" => "→ 少数事業者が市場を支配しており、賃金交渉力が偏っている".into(),
        "HS-6" => "→ 通勤圏が限られている。近隣都市への掲載エリア拡大が即効性あり".into(),
        "FC-1" => "→ 求人数の増減トレンドは今後の競争環境を左右する".into(),
        "FC-2" => "→ 最低賃金との乖離が拡大すると法令リスクが生じる".into(),
        "FC-3" => "→ 生産年齢人口の減少は構造的問題。短期的な対策では解決しない".into(),
        "FC-4" => "→ 求人の長期掲載が常態化すると採用コストが増大する".into(),
        "RC-1" => "→ 地域内での相対的な位置づけを把握し、改善すべき軸を特定する".into(),
        "RC-2" => "→ 給与格差は人材流出の直接的な原因になる".into(),
        "RC-3" => "→ 求人密度は競争の激しさを示す指標".into(),
        "CZ-1" => "→ 通勤圏内の人口規模は潜在的な採用プールの上限を示す".into(),
        "CZ-2" => "→ 周辺地域との給与差が人材の流出入を決定する".into(),
        "CZ-3" => "→ 通勤圏全体の高齢化は中長期的な労働力減少に直結する".into(),
        "CF-1" => "→ 距離ベースのポテンシャルと実際の通勤フローの差が地理的障壁を示す".into(),
        "CF-2" => "→ 最大の流入元を掲載エリアに追加することで応募者プールが拡大する".into(),
        "CF-3" => "→ 地元就業率は地域の自給自足度を示す。低い場合は広域採用が必要".into(),
        "AP-1" => "→ 給与改善は最もインパクトが大きいが、コストも伴う".into(),
        "AP-2" => "→ 求人原稿の改善はコストゼロで即日実行可能".into(),
        "AP-3" => "→ 掲載エリア拡大は低コストで人材プールを広げられる".into(),
        _ => String::new(),
    }
}

/// チャート解釈テキスト（産業別）
pub(crate) fn interpret_industry_chart(data: &[(String, i64)]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let total: i64 = data.iter().map(|(_, c)| c).sum();
    let top = &data[0];
    let top3_sum: i64 = data.iter().take(3).map(|(_, c)| c).sum();
    let top3_pct = if total > 0 {
        top3_sum as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    format!(
        "{}が最多で{}件。上位3産業で全体の{:.0}%を占める。{}",
        top.0,
        super::super::helpers::format_number(top.1),
        top3_pct,
        if top3_pct > 60.0 {
            "特定産業への依存度が高い市場構造。"
        } else {
            "産業が分散した安定的な市場構造。"
        }
    )
}

/// チャート解釈テキスト（人口ピラミッド）
pub(crate) fn interpret_pyramid(
    pyramid: &[std::collections::HashMap<String, serde_json::Value>],
) -> String {
    if pyramid.is_empty() {
        return String::new();
    }

    let mut peak_age = String::new();
    let mut peak_pop: i64 = 0;
    let mut _working_age: i64 = 0;
    let mut elderly: i64 = 0;
    let mut total: i64 = 0;

    for row in pyramid {
        let age = super::super::helpers::get_str_ref(row, "age_group");
        let pop = super::super::helpers::get_i64(row, "male_count")
            + super::super::helpers::get_i64(row, "female_count");
        total += pop;
        if pop > peak_pop {
            peak_pop = pop;
            peak_age = age.to_string();
        }
        match age {
            "15-19" | "20-24" | "25-29" | "30-34" | "35-39" | "40-44" | "45-49" | "50-54"
            | "55-59" | "60-64" | "10-19" | "20-29" | "30-39" | "40-49" | "50-59" | "60-69" => {
                _working_age += pop
            }
            _ => {}
        }
        match age {
            "65-69" | "70-74" | "75-79" | "80-84" | "85+" | "70-79" | "80+" => elderly += pop,
            _ => {}
        }
    }

    let aging_rate = if total > 0 {
        elderly as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    format!(
        "{}歳が最多層（{}人）。高齢化率{:.1}%。{}",
        peak_age,
        super::super::helpers::format_number(peak_pop),
        aging_rate,
        if aging_rate > 30.0 {
            "高齢化が深刻で、労働力確保に中長期的な課題がある。"
        } else if aging_rate > 25.0 {
            "高齢化が進行中。将来の労働力減少に注意が必要。"
        } else {
            "比較的若い人口構成。"
        }
    )
}

/// Top3の1行要約を生成
pub(crate) fn extract_top_findings(insights: &[Insight]) -> Vec<String> {
    insights
        .iter()
        .filter(|i| i.severity <= Severity::Warning)
        .take(3)
        .map(|i| {
            let key_num = i
                .evidence
                .first()
                .map(|e| format!("({})", e.context))
                .unwrap_or_default();
            format!("{} {}", i.title, key_num)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::fetch::InsightContext;
    use super::*;

    // ======== テストヘルパー ========

    /// 空の InsightContext を生成（全フィールド0/空）
    fn mock_empty_ctx() -> InsightContext {
        InsightContext {
            vacancy: vec![],
            resilience: vec![],
            transparency: vec![],
            temperature: vec![],
            competition: vec![],
            cascade: vec![],
            salary_comp: vec![],
            monopsony: vec![],
            spatial_mismatch: vec![],
            wage_compliance: vec![],
            region_benchmark: vec![],
            text_quality: vec![],
            ts_counts: vec![],
            ts_vacancy: vec![],
            ts_salary: vec![],
            ts_fulfillment: vec![],
            ts_tracking: vec![],
            ext_job_ratio: vec![],
            ext_labor_stats: vec![],
            ext_min_wage: vec![],
            ext_turnover: vec![],
            ext_population: vec![],
            ext_pyramid: vec![],
            ext_migration: vec![],
            ext_daytime_pop: vec![],
            ext_establishments: vec![],
            ext_business_dynamics: vec![],
            ext_care_demand: vec![],
            ext_household_spending: vec![],
            ext_climate: vec![],
            // Impl-3: ライフスタイル特性 (P-1/P-2)
            ext_social_life: vec![],
            ext_internet_usage: vec![],
            // Phase A: SSDSE-A 新規6テーブル
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: vec![],
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            ext_education: vec![],
            ext_industry_employees: vec![],
            hw_industry_counts: vec![],
            // Phase A: 県平均
            pref_avg_unemployment_rate: None,
            pref_avg_single_rate: None,
            pref_avg_physicians_per_10k: None,
            pref_avg_daycare_per_1k_children: None,
            pref_avg_habitable_density: None,
            // Phase B: Agoop 人流
            flow: None,
            commute_zone_count: 0,
            commute_zone_pref_count: 0,
            commute_zone_total_pop: 0,
            commute_zone_working_age: 0,
            commute_zone_elderly: 0,
            commute_inflow_total: 0,
            commute_outflow_total: 0,
            commute_self_rate: 0.0,
            commute_inflow_top3: vec![],
            pref: String::new(),
            muni: String::new(),
        }
    }

    /// vacancy_rate を指定した InsightContext を生成
    fn mock_ctx_with_vacancy(vacancy_rate: f64) -> InsightContext {
        use serde_json::Value;
        use std::collections::HashMap;
        let mut ctx = mock_empty_ctx();
        let mut row: HashMap<String, Value> = HashMap::new();
        row.insert("emp_group".to_string(), Value::String("正社員".to_string()));
        row.insert(
            "vacancy_rate".to_string(),
            Value::Number(serde_json::Number::from_f64(vacancy_rate).unwrap()),
        );
        ctx.vacancy.push(row);
        ctx
    }

    /// テスト用 Insight 作成
    fn mock_insight(
        id: &str,
        category: InsightCategory,
        severity: Severity,
        title: &str,
    ) -> Insight {
        Insight {
            id: id.to_string(),
            category,
            severity,
            title: title.to_string(),
            body: "test body".to_string(),
            evidence: vec![],
            related_tabs: vec![],
        }
    }

    // ======== C. レポートテキスト生成テスト ========

    #[test]
    fn test_generate_executive_summary_text_zero_insights() {
        // insights空 + 空InsightContext → パニックせず何らかの文字列を返す
        let insights: Vec<Insight> = vec![];
        let ctx = mock_empty_ctx();
        let result = generate_executive_summary_text(&insights, &ctx);
        assert!(!result.is_empty(), "空insightsでも何らかの文字列を返すべき");
        // 「標準的な状態」判定に落ちる（critical=0, warning=0, positive=0）
        assert!(
            result.contains("標準的な状態"),
            "insights空時は「標準的な状態です」が含まれる: {}",
            result
        );
    }

    #[test]
    fn test_generate_executive_summary_text_with_critical_insights() {
        // Critical 3件 + vacancy_rate=0.35 → 「深刻」が含まれる
        let insights = vec![
            mock_insight(
                "HS-1",
                InsightCategory::HiringStructure,
                Severity::Critical,
                "欠員深刻",
            ),
            mock_insight(
                "HS-2",
                InsightCategory::HiringStructure,
                Severity::Critical,
                "給与低迷",
            ),
            mock_insight(
                "FC-1",
                InsightCategory::Forecast,
                Severity::Critical,
                "減少トレンド",
            ),
        ];
        let ctx = mock_ctx_with_vacancy(0.35);
        let result = generate_executive_summary_text(&insights, &ctx);
        // critical>=3 && vacancy_rate>0.30 → 「深刻な課題を抱えています」
        assert!(
            result.contains("深刻"),
            "Critical 3件+vacancy>0.30時は「深刻」が含まれるべき: {}",
            result
        );
    }

    #[test]
    fn test_generate_chapter_narrative_hiring_structure_empty() {
        // HiringStructure + 空insights → 「特筆すべき事項は検出されませんでした」
        let insights: Vec<&Insight> = vec![];
        let ctx = mock_empty_ctx();
        let result = generate_chapter_narrative(&InsightCategory::HiringStructure, &insights, &ctx);
        assert!(
            result.contains("特筆すべき事項は検出されませんでした"),
            "空insights時は特筆なし文言: {}",
            result
        );
    }

    #[test]
    fn test_generate_chapter_narrative_hiring_structure_critical() {
        // HiringStructure + Critical insight → 「構造的課題」「重大」が含まれる
        let critical = mock_insight(
            "HS-1",
            InsightCategory::HiringStructure,
            Severity::Critical,
            "慢性的人材不足",
        );
        let insights: Vec<&Insight> = vec![&critical];
        let ctx = mock_ctx_with_vacancy(0.35);
        let result = generate_chapter_narrative(&InsightCategory::HiringStructure, &insights, &ctx);
        assert!(
            result.contains("構造的課題"),
            "HiringStructure章は「構造的課題」で始まる: {}",
            result
        );
        assert!(
            result.contains("重大"),
            "Critical count は「重大N件」で表示: {}",
            result
        );
    }

    #[test]
    fn test_generate_chapter_narrative_all_four_categories() {
        // 4カテゴリ全てでパニックせず戻り値が返る
        let ctx = mock_empty_ctx();
        let dummy = mock_insight(
            "AP-1",
            InsightCategory::ActionProposal,
            Severity::Info,
            "テスト施策",
        );
        let insights: Vec<&Insight> = vec![&dummy];

        for cat in &[
            InsightCategory::HiringStructure,
            InsightCategory::Forecast,
            InsightCategory::RegionalCompare,
            InsightCategory::ActionProposal,
        ] {
            let result = generate_chapter_narrative(cat, &insights, &ctx);
            assert!(
                !result.is_empty(),
                "カテゴリ {:?} の narrative が空ではないこと",
                cat
            );
        }

        // 空insightsでも4カテゴリ全てOK
        let empty: Vec<&Insight> = vec![];
        for cat in &[
            InsightCategory::HiringStructure,
            InsightCategory::Forecast,
            InsightCategory::RegionalCompare,
            InsightCategory::ActionProposal,
        ] {
            let result = generate_chapter_narrative(cat, &empty, &ctx);
            assert!(
                !result.is_empty(),
                "カテゴリ {:?} の空insights narrative が空ではないこと",
                cat
            );
        }
    }
}
