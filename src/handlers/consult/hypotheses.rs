//! 仮説エンジン (計画書 §11)
//!
//! - 仮説は8カテゴリに限定 (§11.1)
//! - 仮説オブジェクトは supporting/counter evidence_ids・missing_information・
//!   confidence・priority を持つ (§11.2)
//! - 信頼度は §11.3 の3段階を機械判定 (独立根拠数・粒度一致・欠損・反証で採点)。
//!   信頼度は「正しさの確率」ではなく「根拠の厚さ」を表す
//! - TOP5選定: priority × confidence 順 (§12.3)
//!
//! 全仮説は「〜の可能性」表現とし、断定しない (§19.2)。

use serde::{Deserialize, Serialize};

use super::config;
use super::evidence::{granularity, EvidenceStore};
use super::input::ConsultInput;
use super::signals::Signal;

/// 信頼度3段階 (§11.3)。「根拠の厚さ」であり正しさの確率ではない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn label_ja(&self) -> &'static str {
        match self {
            Confidence::High => "高",
            Confidence::Medium => "中",
            Confidence::Low => "低",
        }
    }
}

/// 検証優先度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn label_ja(&self) -> &'static str {
        match self {
            Priority::High => "高",
            Priority::Medium => "中",
            Priority::Low => "低",
        }
    }
}

/// 仮説カテゴリ (§11.1 の8カテゴリに限定)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HypothesisCategory {
    /// 採用目標設計
    GoalDesign,
    /// 市場構造
    MarketStructure,
    /// 採用条件
    Conditions,
    /// 求人訴求
    Appeal,
    /// 集客・媒体
    Sourcing,
    /// 応募後対応
    PostApplication,
    /// 選考・内定
    Selection,
    /// 定着・離職
    Retention,
}

impl HypothesisCategory {
    pub fn label_ja(&self) -> &'static str {
        match self {
            HypothesisCategory::GoalDesign => "採用目標設計",
            HypothesisCategory::MarketStructure => "市場構造",
            HypothesisCategory::Conditions => "採用条件",
            HypothesisCategory::Appeal => "求人訴求",
            HypothesisCategory::Sourcing => "集客・媒体",
            HypothesisCategory::PostApplication => "応募後対応",
            HypothesisCategory::Selection => "選考・内定",
            HypothesisCategory::Retention => "定着・離職",
        }
    }
}

/// 仮説オブジェクト (§11.2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    /// H-001 形式のID
    pub hypothesis_id: String,
    pub category: HypothesisCategory,
    /// 仮説文 (必ず「〜の可能性」表現)
    pub statement: String,
    pub supporting_evidence_ids: Vec<String>,
    pub counter_evidence_ids: Vec<String>,
    /// 面談で確認すべき不足情報
    pub missing_information: Vec<String>,
    pub confidence: Confidence,
    pub priority: Priority,
    /// 面談前は常に unverified (フェーズCで支持/否定/保留に更新)
    pub status: String,
}

/// 信頼度の機械判定 (§11.3)
///
/// - High: 独立した出典が3つ以上同方向 + 粒度が対象と一致 + 反証なし + サンプル充足
/// - Medium: 独立した出典が2つ以上 (一部粒度差・欠損・反証は許容)
/// - Low: 上記未満 (単一データ・代理指標中心・サンプル不足)
pub fn judge_confidence(
    store: &EvidenceStore,
    supporting_ids: &[String],
    counter_ids: &[String],
    sample_sufficient: bool,
) -> Confidence {
    let supporting: Vec<_> = supporting_ids
        .iter()
        .filter_map(|id| store.get(id))
        .collect();
    let mut sources: Vec<&str> = supporting.iter().map(|e| e.source_name.as_str()).collect();
    sources.sort();
    sources.dedup();
    let independent_sources = sources.len();

    // 粒度一致: 対象求人に近い粒度 (今回CSV/市区町村/企業) の証拠のみで構成されているか
    let granularity_match = !supporting.is_empty()
        && supporting.iter().all(|e| {
            matches!(
                e.granularity.as_str(),
                granularity::CSV | granularity::MUNICIPALITY | granularity::COMPANY
            )
        });

    if independent_sources >= config::CONFIDENCE_HIGH_MIN_SOURCES
        && counter_ids.is_empty()
        && granularity_match
        && sample_sufficient
    {
        Confidence::High
    } else if independent_sources >= config::CONFIDENCE_MEDIUM_MIN_SOURCES {
        Confidence::Medium
    } else {
        Confidence::Low
    }
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
            if s.fired {
                out.extend(s.evidence_ids.iter().cloned());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

struct HypothesisDraft {
    category: HypothesisCategory,
    statement: String,
    supporting: Vec<String>,
    counter: Vec<String>,
    missing: Vec<String>,
    priority: Priority,
}

/// 市場側シグナルから仮説を生成し、confidence×priority 順で全件返す。
/// TOP5選定は `top_hypotheses` で行う。
pub fn build_hypotheses(
    input: &ConsultInput,
    signals: &[Signal],
    store: &EvidenceStore,
) -> Vec<Hypothesis> {
    let mut drafts: Vec<HypothesisDraft> = Vec::new();
    let sample_sufficient = !fired(signals, "S-14");

    // 市場構造: 働き手減少
    if fired(signals, "S-07") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::MarketStructure,
            statement: "対象地域の働き手人口が減少していく見込みで、応募母集団の形成が構造的に難しくなっている可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-07", "S-08"]),
            counter: evidence_of(signals, &["S-09", "S-12"]),
            missing: vec![
                "過去1年の応募数推移".to_string(),
                "応募者の居住地域".to_string(),
            ],
            priority: if fired(signals, "S-10") {
                Priority::High
            } else {
                Priority::Medium
            },
        });
    }

    // 市場構造/集客: 転職顕在層が薄い
    if fired(signals, "S-08") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Sourcing,
            statement:
                "転職を今考えている層が薄く、掲載して待つだけでは応募が集まりにくい可能性がある"
                    .to_string(),
            supporting: evidence_of(signals, &["S-08"]),
            counter: evidence_of(signals, &["S-13"]),
            missing: vec![
                "使用中の媒体と配信設定".to_string(),
                "スカウト・ダイレクト施策の実施状況".to_string(),
            ],
            priority: Priority::Medium,
        });
    }

    // 採用条件: 給与が障害
    if fired(signals, "S-02") || (fired(signals, "S-04") && fired(signals, "S-05")) {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Conditions,
            statement: "給与条件が市場相場に対して低めで、応募獲得の障害になっている可能性がある"
                .to_string(),
            supporting: evidence_of(signals, &["S-02", "S-04", "S-05"]),
            counter: evidence_of(signals, &["S-03"]),
            missing: vec![
                "給与の変更可能範囲".to_string(),
                "手当・賞与の構成".to_string(),
                "直近の辞退理由".to_string(),
            ],
            priority: Priority::High,
        });
    }

    // 求人訴求: 給与優位でも掲載が長引く
    if fired(signals, "S-03") && fired(signals, "S-01") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Appeal,
            statement: "給与面の優位があっても掲載が長引く市場であり、給与以外の訴求・露出・職種名の付け方が課題になっている可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-03", "S-01"]),
            counter: vec![],
            missing: vec![
                "求人の表示回数・クリック率".to_string(),
                "求人タイトルと検索されている語の一致状況".to_string(),
            ],
            priority: Priority::High,
        });
    }

    // 集客・媒体: 配信地域が狭い (§11.2 の例に対応)
    if fired(signals, "S-12") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Sourcing,
            statement: "配信地域が狭く、通勤可能な周辺地域の人材へ求人が届いていない可能性がある"
                .to_string(),
            supporting: evidence_of(signals, &["S-12"]),
            counter: vec![],
            missing: vec![
                "応募者居住地".to_string(),
                "媒体別表示回数".to_string(),
                "通勤手当上限".to_string(),
            ],
            priority: if fired(signals, "S-07") {
                Priority::High
            } else {
                Priority::Medium
            },
        });
    }

    // 定着・離職: 欠員補充型採用の存在 (市場観測)
    if fired(signals, "S-06") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Retention,
            statement: "この地域・職種では人員減少と募集継続が並行する企業が観測され、離職起因の欠員補充採用が発生している可能性がある。自社でも採用理由 (増員/欠員) と離職状況の確認が必要".to_string(),
            supporting: evidence_of(signals, &["S-06"]),
            counter: vec![],
            missing: vec![
                "今回の採用理由 (増員・欠員・新拠点)".to_string(),
                "直近1年の離職数と離職理由".to_string(),
            ],
            priority: Priority::Medium,
        });
    }

    // 市場構造: 求人倍率が高く比較される
    if fired(signals, "S-10") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::MarketStructure,
            statement: "求人倍率が高く、応募者が複数社を比較しながら動いている可能性がある。条件面と対応スピードの両方が比較対象になっている可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-10", "S-01"]),
            counter: evidence_of(signals, &["S-11"]),
            missing: vec![
                "応募から初回連絡までの時間".to_string(),
                "選考日数と面接回数".to_string(),
            ],
            priority: Priority::High,
        });
    }

    // 応募後対応: 市場が緩やかでも掲載が長い
    if fired(signals, "S-11") && fired(signals, "S-01") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::PostApplication,
            statement: "市場は比較的緩やかであるのに掲載が長引いており、応募後の対応 (連絡速度・面接設定) や選考運用側に課題がある可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-11", "S-01"]),
            counter: vec![],
            missing: vec![
                "月間応募数と接触率".to_string(),
                "初回連絡までの時間".to_string(),
                "面接設定率・実施率".to_string(),
            ],
            priority: Priority::Medium,
        });
    }

    // 採用目標設計: 目標が市場供給力に対して高い可能性
    if input.client.hiring_count.is_some()
        && (fired(signals, "S-07") || fired(signals, "S-10") || fired(signals, "S-08"))
    {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::GoalDesign,
            statement: "採用目標 (人数・期限) が市場の供給力に対して高めの可能性があり、目標の分割・期限の見直し・採用手法の追加が論点になる可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-07", "S-08", "S-10"]),
            counter: evidence_of(signals, &["S-09", "S-11"]),
            missing: vec![
                "採用人数の内訳と優先順位".to_string(),
                "期限の背景 (事業計画・欠員補充等)".to_string(),
            ],
            priority: Priority::Medium,
        });
    }

    // 市場構造: 特定企業の大量募集
    if fired(signals, "S-15") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::MarketStructure,
            statement: "特定企業の大量募集が地域の応募母集団を吸収している可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-15"]),
            counter: vec![],
            missing: vec!["当該企業と自社の条件比較".to_string()],
            priority: Priority::Low,
        });
    }

    // 集客・媒体: 新着投入が速い市場
    if fired(signals, "S-13") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Sourcing,
            statement: "新規求人の投入が速い市場で、掲載後の初動対応と掲載の更新頻度が応募獲得を左右している可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-13"]),
            counter: vec![],
            missing: vec![
                "掲載の更新・リフレッシュ頻度".to_string(),
            ],
            priority: Priority::Low,
        });
    }

    // ---- 拡充経路 (2026-07-10): 新シグナルから追加の仮説を生成 ----

    // 市場構造: 人口の構造的な細り (転出超過・自然減・廃業超過のいずれか)
    if fired(signals, "S-16") || fired(signals, "S-23") || fired(signals, "S-18") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::MarketStructure,
            statement: "地域の人口・事業所が構造的に細っており、地元だけを母集団にした採用は中長期で難しくなっていく可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-16", "S-23", "S-18", "S-07"]),
            counter: evidence_of(signals, &["S-09", "S-12", "S-19", "S-21"]),
            missing: vec![
                "採用対象の想定居住範囲".to_string(),
                "過去数年の応募元地域の変化".to_string(),
            ],
            priority: if fired(signals, "S-16") && fired(signals, "S-10") {
                Priority::High
            } else {
                Priority::Medium
            },
        });
    }

    // 集客・媒体: 昼間流出型/通勤流入 → 配信圏の設計
    if fired(signals, "S-17") || fired(signals, "S-28") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Sourcing,
            statement: "人の動き (昼間流出・通勤流入) に対して求人の配信圏や勤務地の見せ方が合っておらず、通える人材に届いていない可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-17", "S-28", "S-12"]),
            counter: vec![],
            missing: vec![
                "求人の配信対象地域の設定".to_string(),
                "応募者の居住地の内訳".to_string(),
                "最寄り駅・通勤手段の記載有無".to_string(),
            ],
            priority: Priority::Medium,
        });
    }

    // 採用条件: 生活コスト (家賃) に対する給与
    if fired(signals, "S-22") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Conditions,
            statement: "地域の家賃等の生活コストに対して市場給与帯が重く、転居を伴う採用や遠方からの応募が集まりにくい可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-22", "S-02"]),
            counter: evidence_of(signals, &["S-03"]),
            missing: vec![
                "採用対象が転居前提か通勤前提か".to_string(),
                "住宅手当・寮などの支援の有無".to_string(),
            ],
            priority: Priority::Medium,
        });
    }

    // 求人訴求: 休日・タグの見せ方が弱い
    if fired(signals, "S-24") || fired(signals, "S-25") || fired(signals, "S-26") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Appeal,
            statement: "求人カード上で観測できる条件の見せ方 (年間休日の明示・訴求タグの厚み) が横並びで、条件比較の土俵で埋もれている可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-24", "S-25", "S-26"]),
            counter: vec![],
            missing: vec![
                "求人原稿に記載している条件の一覧".to_string(),
                "年間休日・休日の取りやすさの実態".to_string(),
            ],
            priority: Priority::Medium,
        });
    }

    // 市場構造: 拡大採用型の競合が地域人材を吸収
    if fired(signals, "S-29") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::MarketStructure,
            statement: "人員を増やしながら募集を続ける拡大採用型の企業があり、地域の応募母集団を積極的に吸収している可能性がある".to_string(),
            supporting: evidence_of(signals, &["S-29"]),
            counter: vec![],
            missing: vec!["当該企業と自社の条件・訴求の差".to_string()],
            priority: Priority::Low,
        });
    }

    // 採用条件: 非正規比率が高い市場での正社員採用
    if fired(signals, "S-30") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Conditions,
            statement: "市場は正社員以外の求人比率が高く、正社員採用であれば雇用の安定を差別化材料にできる可能性がある (逆に非正規採用では競合が多い可能性)".to_string(),
            supporting: evidence_of(signals, &["S-30"]),
            counter: vec![],
            missing: vec![
                "今回の募集の雇用形態".to_string(),
                "正社員登用・キャリアパスの整備状況".to_string(),
            ],
            priority: Priority::Low,
        });
    }

    // draft → Hypothesis (信頼度は機械判定)
    let mut hypotheses: Vec<Hypothesis> = drafts
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            let confidence = judge_confidence(store, &d.supporting, &d.counter, sample_sufficient);
            Hypothesis {
                hypothesis_id: format!("H-{:03}", i + 1),
                category: d.category,
                statement: d.statement,
                supporting_evidence_ids: d.supporting,
                counter_evidence_ids: d.counter,
                missing_information: d.missing,
                confidence,
                priority: d.priority,
                status: "unverified".to_string(),
            }
        })
        .collect();

    // priority desc → confidence desc → id asc で安定ソート
    hypotheses.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(b.confidence.cmp(&a.confidence))
            .then(a.hypothesis_id.cmp(&b.hypothesis_id))
    });
    hypotheses
}

/// TOP5 (config::HYPOTHESIS_TOP_N) を選定
pub fn top_hypotheses(all: &[Hypothesis]) -> Vec<Hypothesis> {
    all.iter().take(config::HYPOTHESIS_TOP_N).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::evidence::EvidenceKind;

    fn store_with(entries: &[(&str, &str, &str)]) -> (EvidenceStore, Vec<String>) {
        // entries: (metric, source, granularity)
        let mut store = EvidenceStore::new();
        let mut ids = Vec::new();
        for (metric, source, gran) in entries {
            ids.push(store.add(
                EvidenceKind::Aggregated,
                metric,
                "1",
                "",
                source,
                gran,
                None,
                None,
                "",
            ));
        }
        (store, ids)
    }

    #[test]
    fn confidence_high_requires_three_sources_no_counter_granularity_match() {
        let (store, ids) = store_with(&[
            ("m1", "出典A", granularity::CSV),
            ("m2", "出典B", granularity::MUNICIPALITY),
            ("m3", "出典C", granularity::COMPANY),
        ]);
        assert_eq!(judge_confidence(&store, &ids, &[], true), Confidence::High);
        // 反証があると High にならない
        let (store2, ids2) = store_with(&[
            ("m1", "出典A", granularity::CSV),
            ("m2", "出典B", granularity::MUNICIPALITY),
            ("m3", "出典C", granularity::COMPANY),
            ("counter", "出典D", granularity::CSV),
        ]);
        let counter = vec![ids2[3].clone()];
        assert_eq!(
            judge_confidence(&store2, &ids2[..3], &counter, true),
            Confidence::Medium
        );
        // サンプル不足でも High にならない
        assert_eq!(
            judge_confidence(&store, &ids, &[], false),
            Confidence::Medium
        );
    }

    #[test]
    fn confidence_medium_on_two_sources() {
        let (store, ids) = store_with(&[
            ("m1", "出典A", granularity::CSV),
            ("m2", "出典B", granularity::PREFECTURE), // 粒度差あり
        ]);
        assert_eq!(
            judge_confidence(&store, &ids, &[], true),
            Confidence::Medium
        );
    }

    #[test]
    fn confidence_low_on_single_source() {
        let (store, ids) = store_with(&[("m1", "出典A", granularity::PREFECTURE)]);
        assert_eq!(judge_confidence(&store, &ids, &[], true), Confidence::Low);
        // 同一出典が複数あっても独立根拠は1つ
        let (store2, ids2) = store_with(&[
            ("m1", "出典A", granularity::CSV),
            ("m2", "出典A", granularity::CSV),
        ]);
        assert_eq!(judge_confidence(&store2, &ids2, &[], true), Confidence::Low);
    }

    #[test]
    fn granularity_mismatch_blocks_high() {
        // 3出典あっても県粒度が混ざると High にしない (§11.3: データ粒度が対象求人と一致)
        let (store, ids) = store_with(&[
            ("m1", "出典A", granularity::CSV),
            ("m2", "出典B", granularity::PREFECTURE),
            ("m3", "出典C", granularity::COMPANY),
        ]);
        assert_eq!(
            judge_confidence(&store, &ids, &[], true),
            Confidence::Medium
        );
    }

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
    fn hypotheses_generated_from_fired_signals_only() {
        let (store, ids) = store_with(&[("倍率", "出典A", granularity::PREFECTURE)]);
        let signals = vec![
            make_signal("S-10", true, &[ids[0].as_str()]),
            make_signal("S-07", false, &[]),
        ];
        let input = ConsultInput::default();
        let hyps = build_hypotheses(&input, &signals, &store);
        assert_eq!(hyps.len(), 1);
        assert_eq!(hyps[0].category, HypothesisCategory::MarketStructure);
        assert!(hyps[0].statement.contains("可能性"));
        assert_eq!(hyps[0].status, "unverified");
    }

    #[test]
    fn all_statements_use_possibility_wording() {
        // 全生成経路を発火させ、全 statement が可能性表現であることを確認
        let (store, ids) = store_with(&[("m", "出典A", granularity::CSV)]);
        let eid = ids[0].as_str();
        let all_fired: Vec<Signal> = [
            "S-01", "S-02", "S-03", "S-04", "S-05", "S-06", "S-07", "S-08", "S-09", "S-10", "S-11",
            "S-12", "S-13", "S-15",
        ]
        .iter()
        .map(|id| make_signal(id, true, &[eid]))
        .collect();
        let input = ConsultInput {
            client: crate::handlers::consult::input::ClientInput {
                hiring_count: Some(3),
                ..Default::default()
            },
            ..Default::default()
        };
        let hyps = build_hypotheses(&input, &all_fired, &store);
        assert!(
            hyps.len() >= 8,
            "8カテゴリ相当の仮説が生成される (実際: {})",
            hyps.len()
        );
        for h in &hyps {
            assert!(
                h.statement.contains("可能性") || h.statement.contains("要確認"),
                "断定表現の疑い: {}",
                h.statement
            );
            assert!(
                !h.supporting_evidence_ids.is_empty(),
                "{}: 根拠IDのない仮説は禁止 (§19.1)",
                h.hypothesis_id
            );
        }
    }

    #[test]
    fn top5_selection_respects_priority_then_confidence() {
        let (store, ids) = store_with(&[("m", "出典A", granularity::CSV)]);
        let eid = ids[0].as_str();
        let all_fired: Vec<Signal> = [
            "S-01", "S-02", "S-04", "S-05", "S-06", "S-07", "S-08", "S-10", "S-12", "S-13", "S-15",
        ]
        .iter()
        .map(|id| make_signal(id, true, &[eid]))
        .collect();
        let input = ConsultInput::default();
        let hyps = build_hypotheses(&input, &all_fired, &store);
        let top = top_hypotheses(&hyps);
        assert!(top.len() <= 5);
        // 先頭は priority=High のはず
        assert_eq!(top[0].priority, Priority::High);
        // 並びが priority 降順であること
        for w in top.windows(2) {
            assert!(w[0].priority >= w[1].priority);
        }
    }
}
