//! 工程間で共有する型。Python `run_job_fact_poc` の抽出スキーマと同形。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 不変項目の固定キー(Python 版 extracted_baseline.json と一致)。
pub const FACT_KEYS: [&str; 8] = [
    "salary",
    "working_hours",
    "holidays",
    "work_location",
    "employment_type",
    "insurance",
    "allowances",
    "required_qualifications",
];

/// 抽出された1項目。value と、原文のどこから取ったかの根拠引用。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FactField {
    /// 抽出値(原文の写し。要約・言い換え禁止)。
    #[serde(default)]
    pub value: String,
    /// 根拠引用(原文に一字一句存在しなければリジェクト)。
    #[serde(default)]
    pub evidence_quote: String,
    /// 検証結果: "verified" | "rejected" | "missing"。
    #[serde(default)]
    pub status: String,
}

/// 抽出結果一式(キーは [`FACT_KEYS`])。
pub type ExtractedFacts = BTreeMap<String, FactField>;

/// 検証済み事実をプロンプト注入用のテキストに整形する(verified のみ)。
pub fn facts_to_text(facts: &ExtractedFacts) -> String {
    let mut lines = Vec::new();
    for key in FACT_KEYS {
        if let Some(f) = facts.get(key) {
            if f.status == "verified" && !f.value.trim().is_empty() {
                lines.push(format!("{}: {}", key, f.value.trim()));
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facts_to_text_verified_のみ出力する() {
        let mut facts = ExtractedFacts::new();
        facts.insert(
            "salary".into(),
            FactField {
                value: "192,000円〜195,000円".into(),
                evidence_quote: "192,000円〜195,000円".into(),
                status: "verified".into(),
            },
        );
        facts.insert(
            "holidays".into(),
            FactField {
                value: "捏造された休日".into(),
                evidence_quote: "".into(),
                status: "rejected".into(),
            },
        );
        let text = facts_to_text(&facts);
        assert!(text.contains("salary: 192,000円〜195,000円"));
        assert!(!text.contains("捏造"));
    }
}
