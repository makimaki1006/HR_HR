//! 矛盾検出 (計画書 §10)
//!
//! 複合分析の主要価値は結論の断定ではなく「違和感」の抽出 (§10 冒頭)。
//! 面談前に市場側データだけで検出可能な矛盾パターンを扱う (§10.1 の上4つ相当):
//! - C-01 給与優位 × 継続掲載が長い
//! - C-02 求人多 × 従業員減
//! - C-03 通勤流入多 × 配信地域は不明 (「要確認」形式)
//! - C-04 市場が緩い × 継続掲載
//!
//! 出力は §10.2 の JSON 形式相当 (interpretations 複数 + questions + confidence)。

use serde::{Deserialize, Serialize};

use super::config;
use super::hypotheses::Confidence;
use super::signals::Signal;

/// 矛盾1件 (§10.2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    /// C-001 形式のID
    pub contradiction_id: String,
    pub title: String,
    pub evidence_ids: Vec<String>,
    /// 複数の解釈 (いずれも可能性表現)
    pub interpretations: Vec<String>,
    /// 面談で確認すべき質問
    pub questions: Vec<String>,
    pub confidence: Confidence,
}

fn signal<'a>(signals: &'a [Signal], id: &str) -> Option<&'a Signal> {
    signals.iter().find(|s| s.id == id)
}

fn fired(signals: &[Signal], id: &str) -> bool {
    signal(signals, id).map(|s| s.fired).unwrap_or(false)
}

fn evidence_of(signals: &[Signal], ids: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        if let Some(s) = signal(signals, id) {
            out.extend(s.evidence_ids.iter().cloned());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 市場側シグナルから矛盾を検出する。最大 `config::CONTRADICTION_MAX` 件。
pub fn detect_contradictions(signals: &[Signal]) -> Vec<Contradiction> {
    let mut out = Vec::new();
    let mut seq = 0usize;
    let mut next_id = || {
        seq += 1;
        format!("C-{:03}", seq)
    };

    // C: 給与優位 (S-03) × 継続掲載長い (S-01)
    if fired(signals, "S-03") && fired(signals, "S-01") {
        out.push(Contradiction {
            contradiction_id: next_id(),
            title: "給与は市場上位なのに長期掲載が多い市場".to_string(),
            evidence_ids: evidence_of(signals, &["S-03", "S-01"]),
            interpretations: vec![
                "求人の露出が不足している可能性".to_string(),
                "求人タイトルが求職者の検索語と一致していない可能性".to_string(),
                "給与以外の条件 (休日・勤務時間・通勤) が弱い可能性".to_string(),
            ],
            questions: vec![
                "求人の表示回数とクリック率は確認できますか".to_string(),
                "応募者はどの地域から来ていますか".to_string(),
            ],
            confidence: Confidence::Medium,
        });
    }

    // C: 求人多 (募集継続) × 従業員減 (S-06)
    if fired(signals, "S-06") {
        out.push(Contradiction {
            contradiction_id: next_id(),
            title: "人員減少中でも募集が続く企業の存在".to_string(),
            evidence_ids: evidence_of(signals, &["S-06"]),
            interpretations: vec![
                "欠員補充型の採用が続いている可能性".to_string(),
                "離職が採用を上回っている可能性".to_string(),
                "採用が難航して掲載が長期化している可能性".to_string(),
                "組織改編 (合併・分社) 等で人員数が見かけ上減っている可能性".to_string(),
            ],
            questions: vec![
                "直近1年の入社数と退職数はそれぞれ何名ですか".to_string(),
                "今回の採用は増員と欠員補充のどちらですか".to_string(),
            ],
            confidence: Confidence::Medium,
        });
    }

    // C: 通勤流入多 (S-12) × 配信地域は不明 → 「要確認」形式 (§10.1: 配信地域・勤務地表記の問題)
    if fired(signals, "S-12") {
        out.push(Contradiction {
            contradiction_id: next_id(),
            title: "通勤流入が多い地域だが、募集が通勤圏に届いているかは要確認".to_string(),
            evidence_ids: evidence_of(signals, &["S-12"]),
            interpretations: vec![
                "配信地域が勤務地の市区町村に限定され、通勤圏の人材へ届いていない可能性"
                    .to_string(),
                "勤務地表記が最寄り駅・アクセス情報を欠き、通勤可能性が伝わっていない可能性"
                    .to_string(),
            ],
            questions: vec![
                "求人の配信対象地域はどの範囲に設定していますか (要確認)".to_string(),
                "応募者の居住地はどの市区町村が多いですか".to_string(),
                "車通勤の可否と通勤手当の上限を教えてください".to_string(),
            ],
            confidence: Confidence::Low,
        });
    }

    // C: 市場緩い (S-11) × 継続掲載 (S-01)
    if fired(signals, "S-11") && fired(signals, "S-01") {
        out.push(Contradiction {
            contradiction_id: next_id(),
            title: "市場は比較的緩やかなのに長期掲載が多い".to_string(),
            evidence_ids: evidence_of(signals, &["S-11", "S-01"]),
            interpretations: vec![
                "選考運用 (連絡速度・面接設定) に課題がある可能性".to_string(),
                "ターゲット設定が市場の人材層とずれている可能性".to_string(),
                "原稿の情報量・訴求が不足している可能性".to_string(),
            ],
            questions: vec![
                "応募から初回連絡までの時間はどのくらいですか".to_string(),
                "求める経験・資格の条件は途中で見直しましたか".to_string(),
            ],
            confidence: Confidence::Medium,
        });
    }

    // C: 給与優位 (S-03) × 市場給与が県平均比低い (S-05)
    // 市場全体が低め、かつ提示給与が市場上位 → 給与訴求を明示できていなければ機会損失の可能性
    if fired(signals, "S-03") && fired(signals, "S-05") {
        out.push(Contradiction {
            contradiction_id: next_id(),
            title: "低め相場の市場で給与上位のポジションにある".to_string(),
            evidence_ids: evidence_of(signals, &["S-03", "S-05"]),
            interpretations: vec![
                "給与優位を求人上で明示できていなければ、強みが伝わっていない可能性".to_string(),
                "給与表記の形式 (幅・手当込み表記) が比較されにくくしている可能性".to_string(),
            ],
            questions: vec![
                "現在の求人原稿で給与はどのように表記していますか".to_string(),
                "給与以外に訴求している条件は何ですか".to_string(),
            ],
            confidence: Confidence::Medium,
        });
    }

    out.truncate(config::CONTRADICTION_MAX);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signal(id: &str, fired: bool, evidence: &[&str]) -> Signal {
        Signal {
            id: id.to_string(),
            name: format!("test {}", id),
            fired,
            evidence_ids: evidence.iter().map(|s| s.to_string()).collect(),
            interpretation: String::new(),
            alternative_explanations: vec![],
            data_note: String::new(),
        }
    }

    #[test]
    fn no_contradictions_when_nothing_fired() {
        let signals = vec![
            make_signal("S-01", false, &["E-001"]),
            make_signal("S-03", false, &["E-002"]),
        ];
        assert!(detect_contradictions(&signals).is_empty());
    }

    #[test]
    fn salary_advantage_with_long_postings_detected() {
        let signals = vec![
            make_signal("S-01", true, &["E-001"]),
            make_signal("S-03", true, &["E-002"]),
        ];
        let out = detect_contradictions(&signals);
        assert_eq!(out.len(), 1);
        let c = &out[0];
        assert!(c.title.contains("給与は市場上位"));
        assert_eq!(c.evidence_ids, vec!["E-001", "E-002"]);
        assert!(c.interpretations.len() >= 2, "解釈は複数保持する (§10.2)");
        assert!(!c.questions.is_empty());
    }

    #[test]
    fn employee_decline_contradiction_detected() {
        let signals = vec![make_signal("S-06", true, &["E-010", "E-011"])];
        let out = detect_contradictions(&signals);
        assert_eq!(out.len(), 1);
        assert!(out[0].title.contains("人員減少中"));
        assert!(out[0]
            .interpretations
            .iter()
            .any(|i| i.contains("組織改編")));
    }

    #[test]
    fn commute_inflow_is_confirmation_form() {
        let signals = vec![make_signal("S-12", true, &["E-020"])];
        let out = detect_contradictions(&signals);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].title.contains("要確認"),
            "配信地域は不明のため要確認形式にする: {}",
            out[0].title
        );
        assert_eq!(out[0].confidence, Confidence::Low);
    }

    #[test]
    fn loose_market_with_long_postings_detected() {
        let signals = vec![
            make_signal("S-01", true, &["E-001"]),
            make_signal("S-11", true, &["E-030"]),
        ];
        let out = detect_contradictions(&signals);
        assert_eq!(out.len(), 1);
        assert!(out[0].title.contains("緩やか"));
    }

    #[test]
    fn contradiction_ids_are_sequential_and_capped() {
        let signals = vec![
            make_signal("S-01", true, &["E-001"]),
            make_signal("S-03", true, &["E-002"]),
            make_signal("S-05", true, &["E-003"]),
            make_signal("S-06", true, &["E-004"]),
            make_signal("S-11", true, &["E-005"]),
            make_signal("S-12", true, &["E-006"]),
        ];
        let out = detect_contradictions(&signals);
        assert!(out.len() <= super::config::CONTRADICTION_MAX);
        for (i, c) in out.iter().enumerate() {
            assert_eq!(c.contradiction_id, format!("C-{:03}", i + 1));
        }
    }
}
