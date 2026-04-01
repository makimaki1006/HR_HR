//! レポートJSON生成（4章ストーリー構成）

use serde_json::{json, Value};
use super::helpers::*;

/// レポートJSON構築
pub fn build_report_json(insights: &[Insight], pref: &str, muni: &str) -> Value {
    let location = if !muni.is_empty() {
        format!("{} {}", pref, muni)
    } else if !pref.is_empty() {
        pref.to_string()
    } else {
        "全国".to_string()
    };

    let executive_summary = generate_executive_summary_text(insights);

    let chapters = vec![
        build_chapter(1, "現状把握 — この地域の求人市場は今どうなっているか",
            insights, InsightCategory::HiringStructure,
            "求人市場の構造的な特徴と課題を分析します。"),
        build_chapter(2, "トレンド分析 — 市場はどこに向かっているか",
            insights, InsightCategory::Forecast,
            "時系列データと人口動態から、今後の市場動向を予測します。"),
        build_chapter(3, "地域ポジショニング — 他地域と比べてどうか",
            insights, InsightCategory::RegionalCompare,
            "同じ都道府県内の他地域との比較から、相対的な位置づけを明らかにします。"),
        build_chapter(4, "推奨アクション — 何をすべきか",
            insights, InsightCategory::ActionProposal,
            "分析結果に基づく具体的な改善施策を、優先度順に提案します。"),
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

/// エグゼクティブサマリー生成
pub(crate) fn generate_executive_summary_text(insights: &[Insight]) -> String {
    let critical = insights.iter().filter(|i| i.severity == Severity::Critical).count();
    let warning = insights.iter().filter(|i| i.severity == Severity::Warning).count();
    let positive = insights.iter().filter(|i| i.severity == Severity::Positive).count();
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

    let mut summary = format!(
        "この地域の求人市場は{}。全{}件の分析指標のうち、\
         重大な課題{}件、注意すべき点{}件、良好な指標{}件が検出されました。",
        overall, total, critical, warning, positive
    );

    // 最も重要な示唆を1文で要約
    if let Some(top) = insights.first() {
        summary.push_str(&format!(" 最も優先度が高い課題は「{}」です。", top.title));
    }

    summary
}

/// 章の構築
fn build_chapter(
    number: u8,
    title: &str,
    all_insights: &[Insight],
    category: InsightCategory,
    intro: &str,
) -> Value {
    let chapter_insights: Vec<&Insight> = all_insights.iter()
        .filter(|i| i.category == category)
        .collect();

    let narrative = if chapter_insights.is_empty() {
        format!("{}現時点では特筆すべき事項は検出されませんでした。", intro)
    } else {
        let mut text = intro.to_string();
        for (i, insight) in chapter_insights.iter().enumerate() {
            if i == 0 {
                text.push_str(&format!("\n\n最も重要な点として、{}。{}", insight.title, insight.body));
            } else {
                text.push_str(&format!("\n\nまた、{}。{}", insight.title, insight.body));
            }
        }
        text
    };

    let insights_json: Vec<Value> = chapter_insights.iter().map(|i| {
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
    }).collect();

    json!({
        "number": number,
        "title": title,
        "narrative": narrative,
        "insight_count": chapter_insights.len(),
        "insights": insights_json,
    })
}

// ======== レポートHTML用ナラティブ生成 ========

/// 採用困難度グレード
pub(crate) struct DifficultyGrade {
    pub letter: &'static str,
    pub label: &'static str,
    pub color: &'static str,
}

/// 採用困難度を算出
pub(crate) fn compute_difficulty_grade(insights: &[Insight], vacancy_rate: f64) -> DifficultyGrade {
    let critical = insights.iter().filter(|i| i.severity == Severity::Critical).count();
    let warning = insights.iter().filter(|i| i.severity == Severity::Warning).count();
    let positive = insights.iter().filter(|i| i.severity == Severity::Positive).count();

    if critical >= 3 && vacancy_rate > 0.30 {
        DifficultyGrade { letter: "D", label: "深刻", color: "#dc2626" }
    } else if critical >= 1 || vacancy_rate > 0.25 {
        DifficultyGrade { letter: "C", label: "困難", color: "#f59e0b" }
    } else if vacancy_rate > 0.20 || warning >= 3 {
        DifficultyGrade { letter: "B", label: "やや困難", color: "#eab308" }
    } else if positive > warning {
        DifficultyGrade { letter: "A", label: "良好", color: "#059669" }
    } else {
        DifficultyGrade { letter: "B-", label: "標準", color: "#6b7280" }
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
    if data.is_empty() { return String::new(); }
    let total: i64 = data.iter().map(|(_, c)| c).sum();
    let top = &data[0];
    let top3_sum: i64 = data.iter().take(3).map(|(_, c)| c).sum();
    let top3_pct = if total > 0 { top3_sum as f64 / total as f64 * 100.0 } else { 0.0 };

    format!(
        "{}が最多で{}件。上位3産業で全体の{:.0}%を占める。{}",
        top.0, super::super::helpers::format_number(top.1), top3_pct,
        if top3_pct > 60.0 { "特定産業への依存度が高い市場構造。" } else { "産業が分散した安定的な市場構造。" }
    )
}

/// チャート解釈テキスト（人口ピラミッド）
pub(crate) fn interpret_pyramid(pyramid: &[std::collections::HashMap<String, serde_json::Value>]) -> String {
    if pyramid.is_empty() { return String::new(); }

    let mut peak_age = String::new();
    let mut peak_pop: i64 = 0;
    let mut working_age: i64 = 0;
    let mut elderly: i64 = 0;
    let mut total: i64 = 0;

    for row in pyramid {
        let age = super::super::helpers::get_str_ref(row, "age_group");
        let pop = super::super::helpers::get_i64(row, "male_count") + super::super::helpers::get_i64(row, "female_count");
        total += pop;
        if pop > peak_pop { peak_pop = pop; peak_age = age.to_string(); }
        match age {
            "15-19"|"20-24"|"25-29"|"30-34"|"35-39"|"40-44"|"45-49"|"50-54"|"55-59"|"60-64"
            | "10-19"|"20-29"|"30-39"|"40-49"|"50-59"|"60-69" => working_age += pop,
            _ => {}
        }
        match age {
            "65-69"|"70-74"|"75-79"|"80-84"|"85+" | "70-79"|"80+" => elderly += pop,
            _ => {}
        }
    }

    let aging_rate = if total > 0 { elderly as f64 / total as f64 * 100.0 } else { 0.0 };
    format!(
        "{}歳が最多層（{}人）。高齢化率{:.1}%。{}",
        peak_age, super::super::helpers::format_number(peak_pop), aging_rate,
        if aging_rate > 30.0 { "高齢化が深刻で、労働力確保に中長期的な課題がある。" }
        else if aging_rate > 25.0 { "高齢化が進行中。将来の労働力減少に注意が必要。" }
        else { "比較的若い人口構成。" }
    )
}

/// Top3の1行要約を生成
pub(crate) fn extract_top_findings(insights: &[Insight]) -> Vec<String> {
    insights.iter()
        .filter(|i| i.severity <= Severity::Warning)
        .take(3)
        .map(|i| {
            let key_num = i.evidence.first()
                .map(|e| format!("({})", e.context))
                .unwrap_or_default();
            format!("{} {}", i.title, key_num)
        })
        .collect()
}
