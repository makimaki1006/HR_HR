//! 「面談の掴み」= この市場の意外な事実 TOP3 (計画書 §12 拡張 / 差別化の核)
//!
//! 商談準備レポートの導入で、コンサルが顧客の想定を裏切る可能性が高い事実を提示するための
//! セクション。顧客の「知っているつもり」を崩し、面談の主導権を握るための素材にする。
//!
//! ## 選定ロジック (ルールベース・決定的)
//! 発火シグナルの中から「インパクトの大きい負値」「意外性のある組合せ」を優先して採点し、
//! 上位3件を選ぶ。LLM は使わない (§18: 数値・選定はコード側で確定)。
//!
//! ## 規律
//! - 断定禁止。各項目は「〜の可能性」「〜という事実」に留め、施策の断定はしない。
//! - 各項目は「そのまま話せる1文 (talk_line)」+ 根拠ID + 「この後につなげる質問」を持つ。
//! - 根拠のない項目は作らない (必ず evidence_ids を伴う)。

use serde::{Deserialize, Serialize};

use super::evidence_pack::ConsultAnalysis;
use super::signals::Signal;

/// 「面談の掴み」1項目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GripItem {
    /// 見出し (短い意外な事実)
    pub headline: String,
    /// そのまま話せる1文 (面談冒頭でコンサルが読める文)
    pub talk_line: String,
    /// 根拠の証拠ID
    pub evidence_ids: Vec<String>,
    /// この後につなげる質問 (顧客の反応を引き出す)
    pub follow_up_question: String,
}

/// 選定候補 (採点前の内部表現)
struct Candidate {
    /// インパクト採点 (大きいほど優先)。負値の大きさ・意外性で決まる。
    score: f64,
    item: GripItem,
}

fn signal<'a>(signals: &'a [Signal], id: &str) -> Option<&'a Signal> {
    signals.iter().find(|s| s.id == id && s.fired)
}

/// 面談の掴み TOP3 を選定する (決定的)。発火シグナルが乏しければ空 (呼び出し側で省略)。
pub fn select_grip_items(analysis: &ConsultAnalysis) -> Vec<GripItem> {
    let signals = &analysis.signals;
    let mut candidates: Vec<Candidate> = Vec::new();

    // 値の絶対値を取り出すヘルパー (証拠の value_text から数値を抜く)。
    let evidence_value = |id: &str| -> Option<f64> {
        analysis
            .evidence
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| parse_leading_number(&e.value_text))
    };

    // 1) 働き手人口の将来減少 (S-07): 負値が大きいほど意外性・インパクト大
    if let Some(s) = signal(signals, "S-07") {
        if let Some(eid) = s.evidence_ids.first() {
            let rate = evidence_value(eid).unwrap_or(0.0); // 例: -18.4
            let magnitude = rate.abs();
            candidates.push(Candidate {
                // 減少率の大きさをそのまま採点 (負の構造変化は掴みになりやすい)
                score: 40.0 + magnitude,
                item: GripItem {
                    headline: "働き手が将来これだけ減る".to_string(),
                    talk_line: format!(
                        "この地域の働き手人口は将来推計で約{:.0}%減る見込みで、今と同じ採り方では母集団そのものが細っていく可能性があります。",
                        magnitude
                    ),
                    evidence_ids: vec![eid.clone()],
                    follow_up_question:
                        "採用のターゲットは地元中心ですか、それとも周辺地域まで広げていますか？"
                            .to_string(),
                },
            });
        }
    }

    // 2) 人員減少中でも募集を続ける同業の存在 (S-06): 「減っているのに募集」という意外な組合せ
    if let Some(s) = signal(signals, "S-06") {
        if !s.evidence_ids.is_empty() {
            candidates.push(Candidate {
                score: 55.0, // 「減っているのに募集」は直感に反するため高採点
                item: GripItem {
                    headline: "人を減らしながら募集を続ける同業がいる".to_string(),
                    talk_line:
                        "この市場には、人員が減っているのに募集を続けている同業が観測されています。欠員補充型の採用が起きている可能性があり、人材の取り合いが起きやすい環境かもしれません。".to_string(),
                    evidence_ids: s.evidence_ids.clone(),
                    follow_up_question:
                        "御社の今回の採用は、増員ですか、それとも退職に伴う欠員補充ですか？"
                            .to_string(),
                },
            });
        }
    }

    // 3) 周辺からの通勤流入が多い (S-12): 「地元だけではない」という気づき
    if let Some(s) = signal(signals, "S-12") {
        if let Some(eid) = s.evidence_ids.first() {
            let inflow = evidence_value(eid).unwrap_or(0.0);
            candidates.push(Candidate {
                // 流入規模を千人単位で採点に反映
                score: 45.0 + inflow / 1000.0,
                item: GripItem {
                    headline: "周辺からこれだけ通勤で人が来ている".to_string(),
                    talk_line: format!(
                        "この地域には周辺市区町村から約{}人が通勤で流入しています。地元だけを配信対象にしていると、通える人材にリーチできていない可能性があります。",
                        format_thousands(inflow as i64)
                    ),
                    evidence_ids: vec![eid.clone()],
                    follow_up_question:
                        "求人の配信対象地域は、勤務地の市区町村だけですか？周辺まで含めていますか？"
                            .to_string(),
                },
            });
        }
    }

    // 4) 転出超過 (S-16): 人が出ていく地域という構造事実
    if let Some(s) = signal(signals, "S-16") {
        if let Some(eid) = s.evidence_ids.first() {
            let rate = evidence_value(eid).unwrap_or(0.0); // 例: -3.2 (‰)
            candidates.push(Candidate {
                score: 30.0 + rate.abs(),
                item: GripItem {
                    headline: "転入より転出が多い地域".to_string(),
                    talk_line:
                        "この地域は転入より転出が多い転出超過の状態で、現役層が流出している構造があります。地元の供給頼みだと、年々母集団が薄くなっていく可能性があります。".to_string(),
                    evidence_ids: vec![eid.clone()],
                    follow_up_question:
                        "直近で、応募が集まりにくくなってきている実感はありますか？".to_string(),
                },
            });
        }
    }

    // 5) 求人倍率が高い (S-10): 売り手市場という事実 (顧客が過小評価しがち)
    if let Some(s) = signal(signals, "S-10") {
        if let Some(eid) = s.evidence_ids.first() {
            let ratio = evidence_value(eid).unwrap_or(0.0);
            candidates.push(Candidate {
                score: 25.0 + ratio * 5.0,
                item: GripItem {
                    headline: "求職者1人を複数社が奪い合っている".to_string(),
                    talk_line: format!(
                        "この地域の有効求人倍率は約{:.2}倍で、求職者1人に対して求人が多い売り手市場です。応募者は複数社を比較しており、条件と対応スピードの両方で見られています。",
                        ratio
                    ),
                    evidence_ids: vec![eid.clone()],
                    follow_up_question:
                        "応募があってから最初に連絡するまで、だいたいどのくらいの時間がかかっていますか？"
                            .to_string(),
                },
            });
        }
    }

    // 6) 正社員以外の求人が多い (S-30): 正社員採用なら差別化余地という意外性
    if let Some(s) = signal(signals, "S-30") {
        if let Some(eid) = s.evidence_ids.first() {
            let share = evidence_value(eid).unwrap_or(0.0);
            candidates.push(Candidate {
                score: 20.0 + share / 5.0,
                item: GripItem {
                    headline: "市場の求人は非正規が中心".to_string(),
                    talk_line: format!(
                        "この市場では求人の約{:.0}%が正社員以外 (パート・契約等) でした。正社員採用であれば、雇用の安定そのものが差別化材料になる可能性があります。",
                        share
                    ),
                    evidence_ids: vec![eid.clone()],
                    follow_up_question:
                        "今回の募集は正社員ですか？その安定性を求人でどう伝えていますか？".to_string(),
                },
            });
        }
    }

    // 採点降順で安定ソートし、TOP3 を返す (同点はシグナル定義順を維持)
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.into_iter().take(3).map(|c| c.item).collect()
}

/// value_text 先頭の数値を取り出す (符号・カンマ・小数対応)。「+24,500人」→ 24500.0 等。
fn parse_leading_number(text: &str) -> Option<f64> {
    let mut buf = String::new();
    for ch in text.chars() {
        if ch == '+' || ch == '-' || ch == '.' || ch.is_ascii_digit() {
            buf.push(ch);
        } else if ch == ',' || ch == ' ' {
            continue;
        } else if !buf.is_empty() {
            break;
        } else if buf.is_empty() {
            // まだ数値開始前。非数値はスキップ
            continue;
        }
    }
    buf.parse::<f64>().ok()
}

/// 3桁区切りの整数表記。
fn format_thousands(n: i64) -> String {
    let neg = n < 0;
    let s = n.abs().to_string();
    let mut out = String::new();
    let bytes = s.as_bytes();
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    if neg {
        format!("-{}", out)
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::evidence_pack::{analyze, tests::rich_input};

    #[test]
    fn parse_leading_number_handles_signs_and_commas() {
        assert_eq!(parse_leading_number("-18.4"), Some(-18.4));
        assert_eq!(parse_leading_number("24,500"), Some(24500.0));
        assert_eq!(parse_leading_number("+3.2"), Some(3.2));
        assert_eq!(parse_leading_number("1.62"), Some(1.62));
        assert_eq!(parse_leading_number("なし"), None);
    }

    #[test]
    fn format_thousands_groups() {
        assert_eq!(format_thousands(24500), "24,500");
        assert_eq!(format_thousands(120), "120");
        assert_eq!(format_thousands(1000000), "1,000,000");
    }

    #[test]
    fn grip_selects_top3_with_evidence_and_wording() {
        let analysis = analyze(&rich_input());
        let items = select_grip_items(&analysis);
        assert!(!items.is_empty(), "発火シグナルがあれば掴みは生成される");
        assert!(items.len() <= 3, "TOP3以内");
        for it in &items {
            // 根拠必須 (§19.1)
            assert!(!it.evidence_ids.is_empty(), "掴みには根拠IDが必須");
            for id in &it.evidence_ids {
                assert!(
                    analysis.evidence.iter().any(|e| &e.id == id),
                    "根拠 {} が実在する",
                    id
                );
            }
            // 断定を避け「可能性」または事実提示に留める
            assert!(!it.talk_line.trim().is_empty());
            assert!(!it.follow_up_question.trim().is_empty());
        }
    }

    #[test]
    fn grip_is_deterministic() {
        let analysis = analyze(&rich_input());
        let a = select_grip_items(&analysis);
        let b = select_grip_items(&analysis);
        let ja = serde_json::to_string(&a).unwrap();
        let jb = serde_json::to_string(&b).unwrap();
        assert_eq!(ja, jb, "同一入力から同一の掴みを再生成できる");
    }

    #[test]
    fn grip_empty_when_no_signals_fire() {
        // 欠損多数・シグナル非発火 → 掴みは空 (呼び出し側で省略)
        let analysis = analyze(&crate::handlers::consult::input::ConsultInput {
            pref: "北海道".to_string(),
            as_of: "2026-07-11".to_string(),
            total_postings: 40,
            ..Default::default()
        });
        let items = select_grip_items(&analysis);
        assert!(items.is_empty(), "掴みになる意外な事実がなければ空");
    }
}
