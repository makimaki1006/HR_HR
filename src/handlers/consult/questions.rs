//! 面談質問生成 (計画書 §12.5 / §12.6 / §13.1)
//!
//! - 質問は仮説検証のために生成する (§26-5)
//! - 各質問は 質問文 + 確認目的 + 関連仮説 + 回答別の次質問 (分岐) を持つ
//! - 分岐ツリー (§12.6) はカテゴリごとの静的データとして持つ
//! - ヒアリング必須15項目 (§13.1) を定数化

use serde::{Deserialize, Serialize};

use super::hypotheses::{Hypothesis, HypothesisCategory};

/// 回答分岐 (§12.6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionBranch {
    /// 回答ケース (例: 「表示が少ない」)
    pub answer_case: String,
    /// そのケースで次に深掘りする質問・論点
    pub next_question: String,
}

/// 面談質問 (§12.5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    /// Q-001 形式のID
    pub question_id: String,
    /// 質問文
    pub text: String,
    /// 確認目的
    pub purpose: String,
    /// 関連仮説ID
    pub related_hypothesis_id: String,
    /// 回答別の次質問 (静的分岐)
    pub branches: Vec<QuestionBranch>,
}

/// ヒアリング必須15項目 (§13.1)。ブリーフ末尾にチェックリストとして掲載する。
pub const REQUIRED_HEARING_ITEMS: [&str; 15] = [
    "採用人数",
    "採用期限",
    "採用理由（増員・欠員・新拠点等）",
    "月間応募数",
    "接触数",
    "面接設定数",
    "面接実施数",
    "内定数",
    "承諾数",
    "使用媒体",
    "初回連絡までの時間",
    "辞退理由",
    "現在の最大課題",
    "変更可能な条件",
    "変更不可能な条件",
];

/// 質問テンプレート1件: (質問文, 確認目的, [(回答ケース, 次質問)])
type QuestionTemplate = (
    &'static str,
    &'static str,
    Vec<(&'static str, &'static str)>,
);

/// カテゴリ別の静的質問テンプレート (§12.6 の分岐ツリーを含む)
fn templates_for(category: HypothesisCategory) -> Vec<QuestionTemplate> {
    // (質問文, 確認目的, [(回答ケース, 次質問)])
    match category {
        HypothesisCategory::GoalDesign => vec![(
            "採用人数と期限の背景を教えてください（事業計画・欠員補充など）",
            "採用目標が市場の供給力と整合しているかを確認する",
            vec![
                ("期限が動かせない", "採用手法の追加（紹介・スカウト等）や要件緩和の余地を確認"),
                ("期限に幅がある", "目標の分割（先行1名→残り）と優先順位を確認"),
            ],
        )],
        HypothesisCategory::MarketStructure => vec![(
            "過去1年の応募数の推移と、応募者の年齢層・居住地の傾向を教えてください",
            "市場構造の変化が自社の応募状況に表れているかを確認する",
            vec![
                ("応募が減っている", "減少が始まった時期と、その頃の条件・媒体変更の有無を確認"),
                ("応募は横ばい", "応募の質（要件合致率）の変化を確認"),
                ("把握していない", "媒体管理画面の確認可否と数値取得の段取りを確認"),
            ],
        )],
        HypothesisCategory::Conditions => vec![(
            "給与・休日など条件のうち、変更できる範囲と変更できない範囲を教えてください",
            "条件改善の実行可能性と優先順位を確認する",
            vec![
                ("給与は変更可能", "いくらまで・どの構成（基本給/手当）で変更できるかを確認"),
                ("給与は変更不可", "給与以外（休日・時間・通勤・教育）の改善余地を確認"),
            ],
        )],
        HypothesisCategory::Appeal => vec![(
            "現在の求人原稿で、最も伝えたい強みは何ですか。それは原稿のどこに書かれていますか",
            "強みが求人上で観測可能な形になっているかを確認する",
            vec![
                ("強みが原稿にない", "原稿への反映と表現（タイトル/冒頭/タグ）を検討"),
                ("強みは書いてある", "表示回数・クリック率を確認し、露出側の問題か訴求側の問題かを切り分け"),
            ],
        )],
        HypothesisCategory::Sourcing => vec![(
            "求人の配信対象地域と使用媒体を教えてください。応募者はどの地域から来ていますか",
            "配信範囲が通勤圏をカバーしているか、媒体選定が適切かを確認する",
            vec![
                ("配信が勤務地周辺のみ", "通勤流入元の市区町村への配信拡大と通勤手当の訴求を検討"),
                ("広域配信済み", "表示回数とクリック率を確認し、露出量か訴求内容かを切り分け"),
                ("把握していない", "媒体管理画面の設定確認の段取りを決める"),
            ],
        )],
        HypothesisCategory::PostApplication => vec![(
            "応募が入ってから最初に連絡するまでの時間と、連絡手段を教えてください",
            "応募後の初動対応が機会損失になっていないかを確認する",
            vec![
                ("当日中に連絡", "接触率と面接設定率を確認し、次の段階（面接）を検証"),
                ("翌日以降", "初動の高速化（自動返信・SMS等）の余地を確認"),
                ("把握していない", "応募〜連絡のフローを書き出して確認"),
            ],
        )],
        HypothesisCategory::Selection => vec![(
            "面接の回数・所要日数と、内定を出してから承諾までの状況を教えてください",
            "選考プロセスの長さ・体験が辞退につながっていないかを確認する",
            vec![
                ("面接に来ない", "リマインドの方法と日程調整の柔軟さを確認"),
                ("内定辞退が多い", "辞退理由と競合他社の条件を確認"),
            ],
        )],
        HypothesisCategory::Retention => vec![(
            "今回の採用は増員ですか、欠員補充ですか。欠員の場合、退職された方の理由を教えてください",
            "採用課題の根が採用側にあるのか定着側にあるのかを確認する",
            vec![
                ("欠員補充", "直近1年の離職数・離職理由・在籍年数の傾向を確認"),
                ("増員", "受け入れ体制（教育・シフト）の準備状況を確認"),
            ],
        )],
    }
}

/// 仮説TOP群から質問を生成する。仮説ごとにカテゴリ別テンプレートを割り当てる。
/// 同一カテゴリの仮説が複数ある場合、質問は重複させない。
pub fn generate_questions(top_hypotheses: &[Hypothesis]) -> Vec<Question> {
    let mut out: Vec<Question> = Vec::new();
    let mut used_categories: Vec<HypothesisCategory> = Vec::new();
    for h in top_hypotheses {
        if used_categories.contains(&h.category) {
            continue;
        }
        used_categories.push(h.category);
        for (text, purpose, branches) in templates_for(h.category) {
            let qid = format!("Q-{:03}", out.len() + 1);
            out.push(Question {
                question_id: qid,
                text: text.to_string(),
                purpose: purpose.to_string(),
                related_hypothesis_id: h.hypothesis_id.clone(),
                branches: branches
                    .into_iter()
                    .map(|(case, next)| QuestionBranch {
                        answer_case: case.to_string(),
                        next_question: next.to_string(),
                    })
                    .collect(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::hypotheses::{Confidence, ConfidenceBreakdown, Priority};

    fn hyp(id: &str, category: HypothesisCategory) -> Hypothesis {
        Hypothesis {
            hypothesis_id: id.to_string(),
            category,
            statement: "テストの可能性がある".to_string(),
            supporting_evidence_ids: vec!["E-001".to_string()],
            counter_evidence_ids: vec![],
            missing_information: vec![],
            confidence: Confidence::Medium,
            confidence_breakdown: ConfidenceBreakdown::default(),
            priority: Priority::High,
            status: "unverified".to_string(),
        }
    }

    #[test]
    fn required_hearing_items_are_15() {
        assert_eq!(REQUIRED_HEARING_ITEMS.len(), 15);
        // §13.1 の主要項目が含まれること
        assert!(REQUIRED_HEARING_ITEMS.contains(&"採用人数"));
        assert!(REQUIRED_HEARING_ITEMS.contains(&"初回連絡までの時間"));
        assert!(REQUIRED_HEARING_ITEMS.contains(&"変更不可能な条件"));
    }

    #[test]
    fn every_question_has_purpose_and_hypothesis_link() {
        let hyps = vec![
            hyp("H-001", HypothesisCategory::Sourcing),
            hyp("H-002", HypothesisCategory::Conditions),
        ];
        let qs = generate_questions(&hyps);
        assert_eq!(qs.len(), 2);
        for q in &qs {
            assert!(!q.purpose.is_empty(), "質問に確認目的がある (§24-6)");
            assert!(!q.related_hypothesis_id.is_empty());
            assert!(!q.branches.is_empty(), "回答別の分岐を持つ (§12.6)");
        }
    }

    #[test]
    fn duplicate_categories_do_not_duplicate_questions() {
        let hyps = vec![
            hyp("H-001", HypothesisCategory::MarketStructure),
            hyp("H-002", HypothesisCategory::MarketStructure),
        ];
        let qs = generate_questions(&hyps);
        assert_eq!(qs.len(), 1);
    }

    #[test]
    fn question_ids_sequential() {
        let hyps = vec![
            hyp("H-001", HypothesisCategory::GoalDesign),
            hyp("H-002", HypothesisCategory::Appeal),
            hyp("H-003", HypothesisCategory::Retention),
        ];
        let qs = generate_questions(&hyps);
        for (i, q) in qs.iter().enumerate() {
            assert_eq!(q.question_id, format!("Q-{:03}", i + 1));
        }
    }

    #[test]
    fn all_categories_have_templates() {
        for cat in [
            HypothesisCategory::GoalDesign,
            HypothesisCategory::MarketStructure,
            HypothesisCategory::Conditions,
            HypothesisCategory::Appeal,
            HypothesisCategory::Sourcing,
            HypothesisCategory::PostApplication,
            HypothesisCategory::Selection,
            HypothesisCategory::Retention,
        ] {
            assert!(!templates_for(cat).is_empty());
        }
    }
}
