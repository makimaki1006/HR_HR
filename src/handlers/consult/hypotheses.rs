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

/// 信頼度判定の内訳 (§11.3 の判定根拠を証拠JSONに残す)。
/// 「なぜ 高/中/低 になったか」を再現できるよう、判定に使った要素を保持する。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfidenceBreakdown {
    /// 独立した出典の数 (source_name の異なり数。名寄せ済み証拠を数える)
    pub independent_sources: usize,
    /// 対象求人に近い粒度 (今回CSV/市区町村/企業) のみで構成されているか
    pub granularity_match: bool,
    /// 反証 (counter evidence) の件数
    pub counter_count: usize,
    /// サンプルが充足しているか (S-14 非発火)
    pub sample_sufficient: bool,
}

impl Default for ConfidenceBreakdown {
    fn default() -> Self {
        ConfidenceBreakdown {
            independent_sources: 0,
            granularity_match: false,
            counter_count: 0,
            sample_sufficient: true,
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
    /// 信頼度判定の内訳 (§11.3。判定の透明性のため証拠JSONに残す)
    pub confidence_breakdown: ConfidenceBreakdown,
    pub priority: Priority,
    /// 面談前は常に unverified (フェーズCで支持/否定/保留に更新)
    pub status: String,
}

/// 信頼度の機械判定 (§11.3)
///
/// - High: 独立した出典が3つ以上同方向 + 粒度が対象と一致 + 反証なし + サンプル充足
/// - Medium: 独立した出典が2つ以上 (一部粒度差・欠損・反証は許容)、
///   または単一出典でも「対象に近い粒度 (今回CSV/市区町村/企業) + サンプル充足 + 反証なし」
///   の観測 (§11.3 の趣旨: 対象求人に直接根ざした観測は中程度の厚みとみなす)
/// - Low: 上記未満 (単一の県/全国粒度データ・代理指標中心・サンプル不足)
pub fn judge_confidence(
    store: &EvidenceStore,
    supporting_ids: &[String],
    counter_ids: &[String],
    sample_sufficient: bool,
) -> Confidence {
    judge_confidence_with_breakdown(store, supporting_ids, counter_ids, sample_sufficient).0
}

/// 信頼度を判定し、判定内訳もあわせて返す (§11.3。証拠JSONに内訳を残すため)。
pub fn judge_confidence_with_breakdown(
    store: &EvidenceStore,
    supporting_ids: &[String],
    counter_ids: &[String],
    sample_sufficient: bool,
) -> (Confidence, ConfidenceBreakdown) {
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

    let breakdown = ConfidenceBreakdown {
        independent_sources,
        granularity_match,
        counter_count: counter_ids.len(),
        sample_sufficient,
    };

    let confidence = if independent_sources >= config::CONFIDENCE_HIGH_MIN_SOURCES
        && counter_ids.is_empty()
        && granularity_match
        && sample_sufficient
    {
        Confidence::High
    } else if independent_sources >= config::CONFIDENCE_MEDIUM_MIN_SOURCES {
        Confidence::Medium
    } else if independent_sources >= 1
        && granularity_match
        && sample_sufficient
        && counter_ids.is_empty()
    {
        // §11.3 調整 (2026-07-11): 現状ほぼ全て「低」で識別力がないため、
        // 対象求人に直接根ざした観測 (今回CSV/市区町村/企業粒度) がサンプル充足・反証なしで
        // 揃っている単一出典ケースは「中」に引き上げる。県/全国粒度の単一データは「低」のまま。
        Confidence::Medium
    } else {
        Confidence::Low
    };

    (confidence, breakdown)
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

/// 2つの根拠集合の Jaccard 類似度 (|∩| / |∪|)。両方空なら 0。
fn jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.iter().filter(|x| b.contains(x)).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// 重なり係数 (overlap coefficient) = |∩| / min(|a|,|b|)。両方空なら 0。
/// 一方の根拠集合が他方にほぼ含まれる (包含) ケースを捉える。
/// 例: 「配信地域が狭い」(根拠1件) が「配信圏が合っていない」(根拠2件) に含まれる → 1.0。
fn overlap_coefficient(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.iter().filter(|x| b.contains(x)).count();
    let min_len = a.len().min(b.len());
    if min_len == 0 {
        0.0
    } else {
        inter as f64 / min_len as f64
    }
}

/// 同一カテゴリで根拠集合が大きく重なる仮説をマージする (P1-4)。
///
/// 実例: 「配信地域が狭い」と「人の動きに配信圏が合っていない」は通勤OD同根拠で並ぶため、
/// TOP5 が同種の仮説で埋まってしまう。閾値以上に根拠が重なる同カテゴリ draft を1つに統合し、
/// TOP5 の枠を多様なカテゴリに使えるようにする。
///
/// マージ規則:
/// - 同一カテゴリかつ根拠 Jaccard >= 閾値 のペアを統合
/// - 優先度は高い方を採用
/// - 根拠 (supporting/counter)・不足情報は重複排除して和集合
/// - statement は先発 (入力順で先) を残し、後発の要点を「(関連: …)」として補足
fn merge_overlapping_drafts(drafts: Vec<HypothesisDraft>) -> Vec<HypothesisDraft> {
    /// 統合とみなす根拠の重なり閾値。
    /// Jaccard (集合全体の一致) か overlap 係数 (包含) のいずれかがこの値以上なら統合する。
    /// overlap も見ることで「小さい根拠集合が大きい方に含まれる」ケース (配信地域が狭い ⊂
    /// 配信圏が合っていない) も統合できる。
    const MERGE_THRESHOLD: f64 = 0.6;

    let mut merged: Vec<HypothesisDraft> = Vec::new();
    for d in drafts {
        // 既存の同カテゴリ・高重複 draft を探す (Jaccard か overlap のどちらかが閾値以上)
        let target = merged.iter_mut().find(|m| {
            m.category == d.category
                && (jaccard(&m.supporting, &d.supporting) >= MERGE_THRESHOLD
                    || overlap_coefficient(&m.supporting, &d.supporting) >= MERGE_THRESHOLD)
        });
        match target {
            Some(m) => {
                // 優先度は高い方
                if d.priority > m.priority {
                    m.priority = d.priority;
                }
                // 根拠・反証・不足情報を和集合 (重複排除)
                for id in d.supporting {
                    if !m.supporting.contains(&id) {
                        m.supporting.push(id);
                    }
                }
                for id in d.counter {
                    if !m.counter.contains(&id) {
                        m.counter.push(id);
                    }
                }
                for info in d.missing {
                    if !m.missing.contains(&info) {
                        m.missing.push(info);
                    }
                }
                // statement は先発を残し、後発の観点を補足 (可能性表現は維持される)
                if !m.statement.contains(&d.statement) {
                    m.statement = format!("{}（関連する観点: {}）", m.statement, d.statement);
                }
            }
            None => merged.push(d),
        }
    }
    merged
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

    // 採用条件: 居住コスト (1畳あたり家賃の全国比) が相対的に高い
    if fired(signals, "S-22") {
        drafts.push(HypothesisDraft {
            category: HypothesisCategory::Conditions,
            statement: "地域の居住コスト (1畳あたり家賃) が全国比で相対的に高く、転居を伴う採用や遠方からの応募が集まりにくい可能性がある".to_string(),
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

    // 仮説の重複統合 (§12.3 / P1-4): 同一カテゴリで根拠集合が大きく重なる draft はマージし、
    // TOP5 の枠を多様なカテゴリに使えるようにする。
    let drafts = merge_overlapping_drafts(drafts);

    // draft → Hypothesis (信頼度は機械判定。内訳も保持)
    let mut hypotheses: Vec<Hypothesis> = drafts
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            let (confidence, breakdown) = judge_confidence_with_breakdown(
                store,
                &d.supporting,
                &d.counter,
                sample_sufficient,
            );
            Hypothesis {
                hypothesis_id: format!("H-{:03}", i + 1),
                category: d.category,
                statement: d.statement,
                supporting_evidence_ids: d.supporting,
                counter_evidence_ids: d.counter,
                missing_information: d.missing,
                confidence,
                confidence_breakdown: breakdown,
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
        // entries: (metric, source, granularity)。
        // 各エントリを別証拠として扱いたいテスト用に、値を index で一意化して
        // 名寄せ (dedup) で潰れないようにする (同一指標の dedup 挙動は evidence.rs 側でテスト済み)。
        let mut store = EvidenceStore::new();
        let mut ids = Vec::new();
        for (i, (metric, source, gran)) in entries.iter().enumerate() {
            ids.push(store.add(
                EvidenceKind::Aggregated,
                metric,
                &format!("{}", i + 1),
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
    fn confidence_low_on_single_pref_source() {
        // 単一の県粒度データは「低」のまま (対象求人から遠い粒度)
        let (store, ids) = store_with(&[("m1", "出典A", granularity::PREFECTURE)]);
        assert_eq!(judge_confidence(&store, &ids, &[], true), Confidence::Low);
        // サンプル不足なら CSV 粒度単一でも「低」
        let (store2, ids2) = store_with(&[("m1", "出典A", granularity::CSV)]);
        assert_eq!(
            judge_confidence(&store2, &ids2, &[], false),
            Confidence::Low
        );
    }

    #[test]
    fn confidence_medium_on_single_target_proximate_observation() {
        // §11.3 調整 (2026-07-11): 対象求人に近い粒度 (今回CSV) の単一観測が
        // サンプル充足・反証なしなら「中」。識別力を出すための引き上げ。
        let (store, ids) = store_with(&[
            ("m1", "出典A", granularity::CSV),
            ("m2", "出典A", granularity::CSV), // 同一出典 → 独立根拠は1つだが粒度は近い
        ]);
        assert_eq!(
            judge_confidence(&store, &ids, &[], true),
            Confidence::Medium
        );
        // 反証があれば「低」に戻る
        let (store2, ids2) = store_with(&[
            ("m1", "出典A", granularity::CSV),
            ("c", "出典B", granularity::CSV),
        ]);
        let counter = vec![ids2[1].clone()];
        assert_eq!(
            judge_confidence(&store2, &ids2[..1], &counter, true),
            Confidence::Low
        );
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
        // 全生成経路を発火させ、全 statement が可能性表現であることを確認。
        // 各シグナルに固有の根拠を割り当てる (実際の運用ではシグナルごとに証拠が異なるため、
        // P1-4 の統合で同カテゴリが過剰に潰れないことも同時に確認する)。
        let sig_ids = [
            "S-01", "S-02", "S-03", "S-04", "S-05", "S-06", "S-07", "S-08", "S-09", "S-10", "S-11",
            "S-12", "S-13", "S-15",
        ];
        let entries: Vec<(&str, &str, &str)> = sig_ids
            .iter()
            .map(|_| ("m", "出典A", granularity::CSV))
            .collect();
        let (store, ids) = store_with(&entries);
        let all_fired: Vec<Signal> = sig_ids
            .iter()
            .enumerate()
            .map(|(i, id)| make_signal(id, true, &[ids[i].as_str()]))
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

    #[test]
    fn jaccard_basic() {
        let a = vec!["E-1".to_string(), "E-2".to_string(), "E-3".to_string()];
        let b = vec!["E-1".to_string(), "E-2".to_string(), "E-3".to_string()];
        assert!((jaccard(&a, &b) - 1.0).abs() < 1e-9);
        let c = vec!["E-1".to_string(), "E-2".to_string()];
        // ∩=2, ∪=3 → 0.666...
        assert!((jaccard(&a, &c) - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(jaccard(&a, &[]), 0.0);
    }

    #[test]
    fn overlapping_sourcing_hypotheses_are_merged() {
        // P1-4 実例: 「配信地域が狭い」(S-12) と「人の動きに配信圏が合っていない」(S-17/S-28)
        // は通勤OD同根拠で並ぶため、同カテゴリ (Sourcing) で根拠が大きく重なる → 1つに統合される。
        let (store, ids) = store_with(&[
            (
                "通勤流入",
                "国勢調査 通勤・通学OD",
                granularity::MUNICIPALITY,
            ),
            (
                "昼夜間人口比率",
                "国勢調査 従業地・通学地集計",
                granularity::MUNICIPALITY,
            ),
        ]);
        // S-12 と S-17/S-28 が同じ通勤OD根拠 (ids[0]) を共有する状況を作る
        let signals = vec![
            make_signal("S-12", true, &[ids[0].as_str()]),
            make_signal("S-17", true, &[ids[0].as_str(), ids[1].as_str()]),
            make_signal("S-28", true, &[ids[0].as_str()]),
        ];
        let input = ConsultInput::default();
        let hyps = build_hypotheses(&input, &signals, &store);
        // Sourcing カテゴリの仮説が2つ以上並ばず、統合されて1つになる
        let sourcing_count = hyps
            .iter()
            .filter(|h| h.category == HypothesisCategory::Sourcing)
            .count();
        assert_eq!(
            sourcing_count, 1,
            "根拠が重なる同カテゴリ仮説は統合される (実際: {})",
            sourcing_count
        );
    }

    #[test]
    fn confidence_breakdown_is_recorded() {
        // P1-5: 信頼度判定の内訳が仮説に残る
        let (store, ids) = store_with(&[
            ("m1", "出典A", granularity::CSV),
            ("m2", "出典B", granularity::MUNICIPALITY),
        ]);
        let signals = vec![
            make_signal("S-07", true, &[ids[0].as_str()]),
            make_signal("S-08", true, &[ids[1].as_str()]),
        ];
        let input = ConsultInput::default();
        let hyps = build_hypotheses(&input, &signals, &store);
        let h = hyps
            .iter()
            .find(|h| h.category == HypothesisCategory::MarketStructure)
            .expect("市場構造の仮説");
        assert_eq!(h.confidence_breakdown.independent_sources, 2);
        assert!(h.confidence_breakdown.granularity_match);
        assert_eq!(h.confidence_breakdown.counter_count, 0);
        assert_eq!(h.confidence, Confidence::Medium);
    }
}
