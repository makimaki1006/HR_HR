//! 個社別アクションメモ (計画書 §14。フェーズD)
//!
//! ヒアリング結果 (consult_hearing_results 最新回答) と面談前の仮説・仮説更新を統合し、
//! 顧客へ共有できる整理メモを生成する。
//!
//! ## 位置づけ (§3 / §24)
//! - このメモは**顧客に共有される**。よって断定を避け、中立表現を用いる。
//! - 「社内用」帯は付けない。冒頭に「お打ち合わせ内容と市場データにもとづく整理」と明記する。
//! - 市場データ由来の情報とヒアリング (お打ち合わせ) 由来の情報は出典を区別して表記する (§24-9)。
//! - 確認済み事項 (ヒアリング回答) と仮説 (未確認) はセクションを分ける (§24-3)。
//! - LLM は使わない (顧客向け文書のためルールベースのみ)。
//!
//! ## ボトルネック判定 (§12.6 の分岐ツリー)
//! ヒアリングの 応募→接触→面接設定→面接実施→内定→承諾 のファネル数値から
//! 最も細い段を特定する。数値が「不明/データなし」の段は「計測から始める」提案に切替える。
//! 判定ロジックは純関数 (`judge_bottleneck`) + テスト。
//!
//! ## 優先施策 (§14.3 の8分類)
//! 施策カタログ (`ACTION_CATALOG`) を静的データで持ち、ボトルネック + 支持された仮説から
//! 上位3〜5件を選定する。各施策は必ず1つ以上の根拠ID (§14.4「各施策は必ず1つ以上の根拠IDと
//! 関連付ける」) を持つ。
//!
//! V2ルール: 介護データ・HW由来データは一切参照しない。

use std::collections::BTreeMap;

use crate::handlers::helpers::escape_html;

use super::config;
use super::evidence_pack::ConsultAnalysis;
use super::hearing::{AnswerValue, DYNAMIC_QUESTIONS, HEARING_ITEMS};
use super::hypotheses::HypothesisCategory;
use super::hypothesis_review::{Decision, HypothesisReview};

// =============================================================================
// ファネル段とボトルネック判定 (§12.6 / §14.2-2)
// =============================================================================

/// 採用ファネルの各段 (§13.1 の応募〜承諾)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunnelStage {
    /// 応募 (母集団形成)
    Application,
    /// 接触 (応募者との連絡)
    Contact,
    /// 面接設定
    InterviewSet,
    /// 面接実施
    InterviewDone,
    /// 内定
    Offer,
    /// 承諾
    Acceptance,
}

impl FunnelStage {
    pub fn label_ja(&self) -> &'static str {
        match self {
            FunnelStage::Application => "応募",
            FunnelStage::Contact => "接触",
            FunnelStage::InterviewSet => "面接設定",
            FunnelStage::InterviewDone => "面接実施",
            FunnelStage::Offer => "内定",
            FunnelStage::Acceptance => "承諾",
        }
    }
}

/// ボトルネック判定結果
#[derive(Debug, Clone, PartialEq)]
pub struct Bottleneck {
    /// 最も細いと判定された段。ファネル数値が全く無い場合は None (計測から)。
    pub stage: Option<FunnelStage>,
    /// 計測が始まっておらず判定できない (数値が不明/データなし) 段があるか
    pub measurement_needed: bool,
    /// 中立表現の説明文 (顧客共有可)。
    pub note: String,
}

/// ファネル各段の観測値 (数値が取れた段のみ Some)。
/// 「不明/データなし」は None として扱い、判定から外す。
#[derive(Debug, Clone, Default)]
struct FunnelCounts {
    /// 月間応募数 (Single 回答「0〜2件」「3件以上」を代表値に写像。数値が別途あればそれを優先)
    application: Option<f64>,
    contact: Option<f64>,
    interview_set: Option<f64>,
    interview_done: Option<f64>,
    offer: Option<f64>,
    acceptance: Option<f64>,
    /// いずれかの段が「不明/データなし」だった (計測未整備の可能性)
    any_unknown: bool,
}

/// 回答マップから数値を取り出す。unknown/no_data のときは None + any_unknown フラグ。
fn number_answer(
    answers: &BTreeMap<String, AnswerValue>,
    key: &str,
    any_unknown: &mut bool,
) -> Option<f64> {
    match answers.get(key) {
        Some(av) if av.unknown || av.no_data => {
            *any_unknown = true;
            None
        }
        Some(av) => {
            let v = av.value.trim().replace(',', "");
            if v.is_empty() {
                None
            } else {
                v.parse::<f64>().ok()
            }
        }
        None => None,
    }
}

/// 月間応募数 (Single 回答帯) を代表値に写像する。
/// 「0〜2件」→ 1.0、「3件以上」→ 5.0 (段間比較用の代表値。実数ではないため note で明示)。
fn application_from_band(
    answers: &BTreeMap<String, AnswerValue>,
    any_unknown: &mut bool,
) -> Option<f64> {
    match answers.get("q04_applications_monthly") {
        Some(av) if av.unknown || av.no_data => {
            *any_unknown = true;
            None
        }
        Some(av) => match av.value.trim() {
            "0〜2件" => Some(1.0),
            "3件以上" => Some(5.0),
            "" => None,
            _ => None,
        },
        None => None,
    }
}

fn funnel_counts(answers: &BTreeMap<String, AnswerValue>) -> FunnelCounts {
    let mut any_unknown = false;
    let application = application_from_band(answers, &mut any_unknown);
    let contact = number_answer(answers, "q05_contacts", &mut any_unknown);
    let interview_set = number_answer(answers, "q06_interviews_set", &mut any_unknown);
    let interview_done = number_answer(answers, "q07_interviews_done", &mut any_unknown);
    let offer = number_answer(answers, "q08_offers", &mut any_unknown);
    let acceptance = number_answer(answers, "q09_acceptances", &mut any_unknown);
    FunnelCounts {
        application,
        contact,
        interview_set,
        interview_done,
        offer,
        acceptance,
        any_unknown,
    }
}

/// ファネル数値から最も細い段 (通過率が最も低い段) を特定する純関数。
///
/// 判定方針:
/// - 隣接する段の通過率 (下段 / 上段) を計算し、最も低い通過率の「下段」をボトルネックとする。
/// - 数値が取れない段が挟まると通過率を計算できないため、その区間はスキップする
///   (取れている段だけで比較する)。
/// - ファネル数値が1つも取れない場合は stage=None + measurement_needed=true とし、
///   「まず計測から始める」提案に切り替える。
/// - 応募数そのものが少ない (代表値が最小の応募段しか無い) 場合は応募段をボトルネックとする。
pub fn judge_bottleneck(answers: &BTreeMap<String, AnswerValue>) -> Bottleneck {
    let f = funnel_counts(answers);

    // 段を順序どおりに (段, 値) で並べる
    let stages: [(FunnelStage, Option<f64>); 6] = [
        (FunnelStage::Application, f.application),
        (FunnelStage::Contact, f.contact),
        (FunnelStage::InterviewSet, f.interview_set),
        (FunnelStage::InterviewDone, f.interview_done),
        (FunnelStage::Offer, f.offer),
        (FunnelStage::Acceptance, f.acceptance),
    ];

    // 数値が取れた段だけ抽出
    let present: Vec<(FunnelStage, f64)> = stages
        .iter()
        .filter_map(|(st, v)| v.map(|x| (*st, x)))
        .collect();

    // ファネル数値が1つも無い → 計測から
    if present.is_empty() {
        return Bottleneck {
            stage: None,
            measurement_needed: true,
            note: "応募から承諾までの各段の件数がまだ十分に把握できていないため、まずは各段の件数を継続的に記録することから始めるのが有効と考えられます。".to_string(),
        };
    }

    // 応募段しか無い、かつ応募が少ない帯 (代表値1.0) → 応募段がボトルネック
    if present.len() == 1 {
        let (st, v) = present[0];
        if st == FunnelStage::Application && v <= 1.0 {
            return Bottleneck {
                stage: Some(FunnelStage::Application),
                measurement_needed: f.any_unknown,
                note: with_measurement_suffix(
                    "現時点で把握できている範囲では、応募の母集団形成の段が最も課題になりやすい段と考えられます。".to_string(),
                    f.any_unknown,
                ),
            };
        }
    }

    // 隣接段 (連続して数値が取れている区間) の通過率を計算し、最小の下段をボトルネックに
    let mut worst_stage: Option<FunnelStage> = None;
    let mut worst_rate = f64::INFINITY;
    for w in present.windows(2) {
        let (_upper_stage, upper) = w[0];
        let (lower_stage, lower) = w[1];
        if upper > 0.0 {
            let rate = lower / upper;
            if rate < worst_rate {
                worst_rate = rate;
                worst_stage = Some(lower_stage);
            }
        } else if lower == 0.0 {
            // 上段が0で下段も0 → その下段を候補 (通過率0扱い)
            if worst_rate > 0.0 {
                worst_rate = 0.0;
                worst_stage = Some(lower_stage);
            }
        }
    }

    // 隣接段の比較が1つも作れなかった (段が飛び飛び) 場合は、
    // 取れている中で最小値の段を控えめにボトルネック候補とする
    let stage = worst_stage.or_else(|| {
        present
            .iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(st, _)| *st)
    });

    let note = match stage {
        Some(st) => with_measurement_suffix(
            format!(
                "現在把握できている件数からは、「{}」の段への通過が相対的に細くなりやすい段と考えられます。",
                st.label_ja()
            ),
            f.any_unknown,
        ),
        None => "各段の件数が飛び飛びで、段ごとの通過状況を十分に比較できていない状況です。".to_string(),
    };

    Bottleneck {
        stage,
        measurement_needed: f.any_unknown,
        note,
    }
}

fn with_measurement_suffix(base: String, any_unknown: bool) -> String {
    if any_unknown {
        format!(
            "{base}なお、一部の段は件数の把握が難しい状況のため、あわせて計測の整備を進めると精度が上がると考えられます。"
        )
    } else {
        base
    }
}

// =============================================================================
// 施策カタログ (§14.3 の8分類)
// =============================================================================

/// 施策分類 (§14.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionCategory {
    /// 条件改善
    Conditions,
    /// 求人訴求
    Appeal,
    /// 集客改善
    Sourcing,
    /// 応募対応改善
    PostApplication,
    /// 選考改善
    Selection,
    /// 内定承諾改善
    OfferAcceptance,
    /// 定着改善
    Retention,
    /// データ計測改善
    Measurement,
}

impl ActionCategory {
    pub fn label_ja(&self) -> &'static str {
        match self {
            ActionCategory::Conditions => "条件改善",
            ActionCategory::Appeal => "求人訴求",
            ActionCategory::Sourcing => "集客改善",
            ActionCategory::PostApplication => "応募対応改善",
            ActionCategory::Selection => "選考改善",
            ActionCategory::OfferAcceptance => "内定承諾改善",
            ActionCategory::Retention => "定着改善",
            ActionCategory::Measurement => "データ計測改善",
        }
    }
}

/// 施策カタログの1件 (静的定義)
pub struct ActionTemplate {
    pub category: ActionCategory,
    /// 施策文 (中立表現。断定しない)
    pub action: &'static str,
    /// この施策の想定KPI (計測できる指標)
    pub kpi: &'static str,
    /// この施策が主に効く仮説カテゴリ (仮説の支持で優先度が上がる)
    pub related_categories: &'static [HypothesisCategory],
    /// この施策が主に効くファネル段 (ボトルネック一致で優先度が上がる)
    pub related_stages: &'static [FunnelStage],
}

/// §14.3 の8分類をカバーする施策カタログ。
/// 各施策は選定後に必ず根拠ID (仮説の根拠 or ボトルネックの根拠) を付与する。
pub const ACTION_CATALOG: &[ActionTemplate] = &[
    // ---- 集客改善 ----
    ActionTemplate {
        category: ActionCategory::Sourcing,
        action: "求人の配信対象地域を通勤圏の周辺市区町村まで広げ、通える人材に届く範囲を見直す",
        kpi: "配信地域数 / 表示回数",
        related_categories: &[HypothesisCategory::Sourcing, HypothesisCategory::MarketStructure],
        related_stages: &[FunnelStage::Application],
    },
    ActionTemplate {
        category: ActionCategory::Sourcing,
        action: "使用媒体と配信設定を棚卸しし、応募の入り口が偏っていないかを確認する",
        kpi: "媒体別の表示回数・応募数",
        related_categories: &[HypothesisCategory::Sourcing],
        related_stages: &[FunnelStage::Application],
    },
    // ---- 求人訴求 ----
    ActionTemplate {
        category: ActionCategory::Appeal,
        action: "求人タイトルと冒頭に、求職者が検索・比較する語や自社の強みを反映する",
        kpi: "クリック率 / 応募数",
        related_categories: &[HypothesisCategory::Appeal],
        related_stages: &[FunnelStage::Application],
    },
    ActionTemplate {
        category: ActionCategory::Appeal,
        action: "求人カード上で見える条件 (休日・手当など) の記載を、比較で埋もれない粒度に整える",
        kpi: "クリック率 / 応募数",
        related_categories: &[HypothesisCategory::Appeal, HypothesisCategory::Conditions],
        related_stages: &[FunnelStage::Application],
    },
    // ---- 条件改善 ----
    ActionTemplate {
        category: ActionCategory::Conditions,
        action: "変更可能な範囲で給与・手当・休日の見せ方を市場帯に合わせて調整する余地を検討する",
        kpi: "応募数 / 内定承諾率",
        related_categories: &[HypothesisCategory::Conditions],
        related_stages: &[FunnelStage::Application, FunnelStage::Acceptance],
    },
    // ---- 応募対応改善 ----
    ActionTemplate {
        category: ActionCategory::PostApplication,
        action: "応募から初回連絡までの時間を短縮する運用 (自動返信・当日連絡の担当決め) を整える",
        kpi: "初回連絡までの時間 / 接触率",
        related_categories: &[HypothesisCategory::PostApplication],
        related_stages: &[FunnelStage::Contact, FunnelStage::InterviewSet],
    },
    ActionTemplate {
        category: ActionCategory::PostApplication,
        action: "面接日程の候補提示とリマインドの手順を決め、面接設定までの離脱を減らす",
        kpi: "接触→面接設定率",
        related_categories: &[HypothesisCategory::PostApplication],
        related_stages: &[FunnelStage::InterviewSet, FunnelStage::InterviewDone],
    },
    // ---- 選考改善 ----
    ActionTemplate {
        category: ActionCategory::Selection,
        action: "面接の回数・所要日数を見直し、選考期間が長すぎて離脱していないかを確認する",
        kpi: "面接設定→実施率 / 選考日数",
        related_categories: &[HypothesisCategory::Selection],
        related_stages: &[FunnelStage::InterviewDone, FunnelStage::Offer],
    },
    // ---- 内定承諾改善 ----
    ActionTemplate {
        category: ActionCategory::OfferAcceptance,
        action: "内定から承諾までのフォロー (条件のすり合わせ・不安点の解消) の手順を整える",
        kpi: "内定→承諾率",
        related_categories: &[HypothesisCategory::Selection, HypothesisCategory::Conditions],
        related_stages: &[FunnelStage::Acceptance],
    },
    // ---- 定着改善 ----
    ActionTemplate {
        category: ActionCategory::Retention,
        action: "直近の離職状況と理由を整理し、欠員補充が繰り返される要因があれば受け入れ体制を見直す",
        kpi: "早期離職数 / 定着率",
        related_categories: &[HypothesisCategory::Retention],
        related_stages: &[],
    },
    // ---- データ計測改善 ----
    ActionTemplate {
        category: ActionCategory::Measurement,
        action: "応募・接触・面接・内定・承諾の各段の件数を継続的に記録し、段ごとの通過状況を見えるようにする",
        kpi: "各段の件数の記録有無",
        related_categories: &[],
        related_stages: &[],
    },
];

/// 選定された施策1件 (根拠ID付き)
#[derive(Debug, Clone)]
pub struct SelectedAction {
    pub category: ActionCategory,
    pub action: String,
    pub kpi: String,
    /// 優先度 (High/Medium/Low)。ボトルネック一致・支持仮説で上がる。
    pub priority: ActionPriority,
    /// 根拠ID (仮説ID or 特別値。§14.4「各施策は必ず1つ以上の根拠ID」)。必ず1件以上。
    pub evidence_refs: Vec<String>,
}

/// 施策の優先度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionPriority {
    Low,
    Medium,
    High,
}

impl ActionPriority {
    pub fn label_ja(&self) -> &'static str {
        match self {
            ActionPriority::High => "高",
            ActionPriority::Medium => "中",
            ActionPriority::Low => "低",
        }
    }
}

/// ボトルネックの根拠として使う擬似ID (ヒアリング由来であることを示す)。
pub const BOTTLENECK_EVIDENCE_REF: &str = "ヒアリング(応募〜承諾の件数)";

/// 支持された仮説カテゴリ集合 (レビュー結果から) を取り出す。
/// 支持 (Support) のみを対象とし、否定・保留は含めない。
fn supported_hypotheses<'a>(
    analysis: &'a ConsultAnalysis,
    reviews: &[HypothesisReview],
) -> Vec<&'a super::hypotheses::Hypothesis> {
    let supported_ids: Vec<&str> = reviews
        .iter()
        .filter(|r| r.decision == Decision::Support)
        .map(|r| r.hypothesis_id.as_str())
        .collect();
    analysis
        .hypotheses
        .iter()
        .filter(|h| supported_ids.contains(&h.hypothesis_id.as_str()))
        .collect()
}

/// 施策を選定する (§14.3。ボトルネック + 支持仮説から上位3〜5件)。
///
/// 各施策には必ず1件以上の根拠IDを付与する:
/// - ボトルネック段に一致した施策 → BOTTLENECK_EVIDENCE_REF
/// - 支持仮説カテゴリに一致した施策 → 当該仮説の supporting_evidence_ids から代表1件 + 仮説ID
/// - どちらにも紐づかない計測改善はボトルネックの計測必要時のみ、BOTTLENECK_EVIDENCE_REF を付与
///
/// 根拠が全く付かない施策は候補から除外する (§14.4 遵守)。
pub fn select_actions(
    analysis: &ConsultAnalysis,
    reviews: &[HypothesisReview],
    bottleneck: &Bottleneck,
) -> Vec<SelectedAction> {
    let supported = supported_hypotheses(analysis, reviews);

    // カテゴリ → 支持仮説 (根拠ID付き) の対応
    let mut candidates: Vec<SelectedAction> = Vec::new();

    for tmpl in ACTION_CATALOG.iter() {
        let mut refs: Vec<String> = Vec::new();
        let mut score: i32 = 0;

        // ボトルネック段一致
        let stage_match = bottleneck
            .stage
            .map(|st| tmpl.related_stages.contains(&st))
            .unwrap_or(false);
        if stage_match {
            score += 3;
            refs.push(BOTTLENECK_EVIDENCE_REF.to_string());
        }

        // 支持仮説カテゴリ一致 → 仮説IDと代表根拠を付与
        for h in &supported {
            if tmpl.related_categories.contains(&h.category) {
                score += 2;
                refs.push(h.hypothesis_id.clone());
                if let Some(first) = h.supporting_evidence_ids.first() {
                    refs.push(first.clone());
                }
            }
        }

        // データ計測改善: ボトルネックが計測必要 or 判定不能のとき優先付与
        if tmpl.category == ActionCategory::Measurement
            && (bottleneck.measurement_needed || bottleneck.stage.is_none())
        {
            score += 4;
            refs.push(BOTTLENECK_EVIDENCE_REF.to_string());
        }

        if refs.is_empty() {
            // 根拠が付かない施策は §14.4 に反するため候補にしない
            continue;
        }

        // 重複根拠を除去 (安定順)
        refs.dedup();
        let mut seen = std::collections::BTreeSet::new();
        refs.retain(|r| seen.insert(r.clone()));

        let priority = if score >= 5 {
            ActionPriority::High
        } else if score >= 3 {
            ActionPriority::Medium
        } else {
            ActionPriority::Low
        };

        candidates.push(SelectedAction {
            category: tmpl.category,
            action: tmpl.action.to_string(),
            kpi: tmpl.kpi.to_string(),
            priority,
            evidence_refs: refs,
        });
    }

    // 優先度降順 → カタログ順 (安定) で上位を採る
    candidates.sort_by(|a, b| b.priority.cmp(&a.priority));

    candidates
        .into_iter()
        .take(config::ACTION_MEMO_MAX_ACTIONS)
        .collect()
}

// =============================================================================
// メモ本文の各セクションデータ (§14.2 の9項目)
// =============================================================================

/// 確認済み事項1件 (ヒアリング回答由来)。
#[derive(Debug, Clone)]
pub struct ConfirmedItem {
    pub label: String,
    pub value: String,
}

/// 顧客共有メモ向けに評価語を中立表現へ置換する (§中立表現)。
///
/// 面談前の仮説文は社内ブリーフ向けに書かれており、「優位」「劣位」「集中」等の評価語を
/// 含むことがある。顧客へ共有するメモではこれらを中立表現に置き換える。
/// 置換は決定的で、意味を保ちつつ評価色を落とす。
pub fn neutralize(text: &str) -> String {
    // 長い語から順に置換する (部分一致の取りこぼしを防ぐ)
    const REPLACEMENTS: &[(&str, &str)] = &[
        ("優位があっても", "が相対的に高めであっても"),
        ("給与面の優位", "給与面の相対的な高さ"),
        ("競争力が高い", "が市場内で相対的に高め"),
        ("優位", "相対的な高さ"),
        ("劣位", "相対的な低さ"),
        ("見劣り", "相対的に控えめ"),
        ("吸収している", "受けている"),
        ("吸収", "受け入れ"),
        ("大量募集", "まとまった募集"),
        ("集中している", "偏りがある"),
        ("集中", "偏り"),
        ("埋もれている", "目立ちにくくなっている"),
        ("縮小", "小さくなる方向"),
    ];
    let mut out = text.to_string();
    for (from, to) in REPLACEMENTS {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out
}

/// ラベルを引く (HEARING_ITEMS / DYNAMIC_QUESTIONS から)。
fn label_of(key: &str) -> String {
    if let Some(it) = HEARING_ITEMS.iter().find(|i| i.key == key) {
        return it.label.to_string();
    }
    if let Some(dq) = DYNAMIC_QUESTIONS.iter().find(|d| d.key == key) {
        return dq.label.to_string();
    }
    key.to_string()
}

/// 回答の表示文字列 (不明/データなしを明示)。
fn answer_display(av: &AnswerValue) -> Option<String> {
    if av.unknown {
        return Some("不明 (顧客が把握していない)".to_string());
    }
    if av.no_data {
        return Some("データなし (計測・記録が存在しない)".to_string());
    }
    let v = av.value.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// 採用目標セクション (§14.2-1)。ヒアリングの人数・期限・理由を拾う。
fn recruiting_goal(answers: &BTreeMap<String, AnswerValue>) -> Vec<ConfirmedItem> {
    let mut out = Vec::new();
    for key in ["q01_hiring_count", "q02_deadline", "q03_reason"] {
        if let Some(av) = answers.get(key) {
            if let Some(disp) = answer_display(av) {
                out.push(ConfirmedItem {
                    label: label_of(key),
                    value: disp,
                });
            }
        }
    }
    out
}

/// 確認済みのファネル・運用系事項 (§14.2 判断根拠の材料)。
fn confirmed_funnel(answers: &BTreeMap<String, AnswerValue>) -> Vec<ConfirmedItem> {
    let mut out = Vec::new();
    for key in [
        "q04_applications_monthly",
        "q05_contacts",
        "q06_interviews_set",
        "q07_interviews_done",
        "q08_offers",
        "q09_acceptances",
        "q11_first_contact_time",
    ] {
        if let Some(av) = answers.get(key) {
            if let Some(disp) = answer_display(av) {
                out.push(ConfirmedItem {
                    label: label_of(key),
                    value: disp,
                });
            }
        }
    }
    out
}

/// 未確認事項 (§14.2-5)。ヒアリングで「不明/データなし」だった項目 + 支持仮説の不足情報。
fn open_items(
    answers: &BTreeMap<String, AnswerValue>,
    analysis: &ConsultAnalysis,
    reviews: &[HypothesisReview],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // ヒアリングで不明/データなしだった項目
    for (key, av) in answers.iter() {
        if av.unknown || av.no_data {
            let tag = if av.unknown {
                "不明"
            } else {
                "データなし"
            };
            out.push(format!("{} ({})", label_of(key), tag));
        }
    }
    // 支持 or 保留の仮説の不足情報 (未確認)
    let relevant_ids: Vec<&str> = reviews
        .iter()
        .filter(|r| r.decision == Decision::Support || r.decision == Decision::Hold)
        .map(|r| r.hypothesis_id.as_str())
        .collect();
    for h in &analysis.hypotheses {
        if relevant_ids.contains(&h.hypothesis_id.as_str()) {
            for m in &h.missing_information {
                if !out.iter().any(|x| x == m) {
                    out.push(m.clone());
                }
            }
        }
    }
    out
}

/// 否定された仮説 (§14.2-4)。
fn rejected_hypotheses<'a>(
    analysis: &'a ConsultAnalysis,
    reviews: &[HypothesisReview],
) -> Vec<(&'a super::hypotheses::Hypothesis, String)> {
    reviews
        .iter()
        .filter(|r| r.decision == Decision::Reject)
        .filter_map(|r| {
            analysis
                .hypotheses
                .iter()
                .find(|h| h.hypothesis_id == r.hypothesis_id)
                .map(|h| (h, r.note.clone()))
        })
        .collect()
}

// =============================================================================
// メモ HTML (§14。顧客共有可。navy スタイル流用・社内用帯なし)
// =============================================================================

/// メモ専用の追加CSS (navy CSS の後に読み込む)。社内用帯は使わない。
fn memo_css() -> &'static str {
    r#"
/* ==== 個社別アクションメモ (顧客共有可) 追加スタイル ==== */
@page {
  @bottom-left {
    content: "お打ち合わせ内容と市場データにもとづく整理";
    font-family: "Noto Sans JP", sans-serif;
    font-size: 8pt;
    color: #6A6E7A;
    letter-spacing: 0.04em;
  }
}
body.theme-navy .memo-intro {
  background: #F4F1E8; border: 1px solid #D8D2C4; border-left: 3px solid #1F2D4D;
  padding: 3mm 4mm; margin-bottom: 4mm; font-size: 9.5pt; line-height: 1.7; color: #333;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy .memo-sec { margin-bottom: 5mm; break-inside: avoid; }
body.theme-navy .memo-sec > h2 {
  font-size: 12pt; color: #1F2D4D; border-bottom: 1.5px solid #1F2D4D;
  padding-bottom: 1mm; margin: 0 0 2.5mm 0;
}
body.theme-navy .memo-sec .memo-no {
  display: inline-block; min-width: 6mm; color: #A8331F; font-weight: 700;
}
body.theme-navy .memo-source-tag {
  display: inline-block; font-size: 7.5pt; font-weight: 700; border-radius: 3px;
  padding: 0 5px; margin-left: 6px; vertical-align: 1px;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy .memo-source-hearing { background: #E7EEF7; color: #1F2D4D; border: 1px solid #1F2D4D; }
body.theme-navy .memo-source-market { background: #EDE7DA; color: #6A5A2A; border: 1px solid #9A8A4A; }
body.theme-navy .memo-kv { width: 100%; border-collapse: collapse; font-size: 9.5pt; }
body.theme-navy .memo-kv th, body.theme-navy .memo-kv td {
  border: 1px solid #D8D2C4; padding: 1.5mm 2.5mm; text-align: left; vertical-align: top;
}
body.theme-navy .memo-kv th { width: 40mm; background: #F4F1E8; color: #1F2D4D; font-weight: 700;
  -webkit-print-color-adjust: exact; print-color-adjust: exact; }
body.theme-navy .memo-bottleneck {
  background: #FBF7EC; border: 1px solid #C9BC94; padding: 2.5mm 3mm; font-size: 10pt;
  line-height: 1.7; -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy .memo-bottleneck strong { color: #1F2D4D; }
body.theme-navy ul.memo-list { margin: 0; padding-left: 5mm; font-size: 9.5pt; line-height: 1.75; }
body.theme-navy .memo-hyp { font-size: 9.5pt; line-height: 1.6; margin-bottom: 1.5mm; }
body.theme-navy .memo-hyp .memo-hyp-note { color: #6A6E7A; }
body.theme-navy table.memo-actions { width: 100%; border-collapse: collapse; font-size: 8.5pt; }
body.theme-navy table.memo-actions th, body.theme-navy table.memo-actions td {
  border: 1px solid #D8D2C4; padding: 1.5mm 2mm; text-align: left; vertical-align: top;
}
body.theme-navy table.memo-actions th {
  background: #1F2D4D; color: #fff; font-weight: 700;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy table.memo-actions .memo-fill {
  min-height: 6mm; background: #FCFBF7; border-bottom: 1px dashed #B8AE96;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy .memo-prio-high { color: #A8331F; font-weight: 700; }
body.theme-navy .memo-prio-mid { color: #6A5A2A; font-weight: 700; }
body.theme-navy .memo-prio-low { color: #6A6E7A; }
body.theme-navy .memo-review-fill {
  min-height: 8mm; border: 1px solid #D8D2C4; background: #FCFBF7; padding: 1.5mm 2mm;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
"#
}

/// 出典タグ (ヒアリング / 市場データ) を区別表記する (§24-9)。
fn source_tag_hearing() -> &'static str {
    r#"<span class="memo-source-tag memo-source-hearing">お打ち合わせ内容</span>"#
}
fn source_tag_market() -> &'static str {
    r#"<span class="memo-source-tag memo-source-market">市場データ</span>"#
}

fn priority_class(p: ActionPriority) -> &'static str {
    match p {
        ActionPriority::High => "memo-prio-high",
        ActionPriority::Medium => "memo-prio-mid",
        ActionPriority::Low => "memo-prio-low",
    }
}

/// 「先にヒアリング入力をしてください」の案内ページ (§14.1 作成条件未達)。
pub fn action_memo_needs_hearing_html(region: &str, session_id: &str) -> String {
    let sid = escape_html(session_id);
    let region_disp = if region.trim().is_empty() {
        "（未特定）".to_string()
    } else {
        escape_html(region)
    };
    let mut html = String::with_capacity(8 * 1024);
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\" data-theme=\"default\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<meta name=\"robots\" content=\"noindex,nofollow\">\n");
    html.push_str("<title>個社別アクションメモ</title>\n<style>\n");
    html.push_str(&crate::handlers::survey::report_html::navy_css_bundle());
    html.push_str(memo_css());
    html.push_str("</style>\n</head>\n<body class=\"theme-navy\">\n");
    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(&format!(
        r#"<div class="page-head">
  <div class="ph-sec">個社別アクションメモ</div>
  <div class="ph-title">{region}</div>
  <div class="ph-rule"></div>
</div>"#,
        region = region_disp
    ));
    html.push_str(&format!(
        r#"<div class="memo-intro">
このメモを作成するには、先に面談で確認した採用状況（ヒアリング）を入力してください。<br>
入力後、お打ち合わせ内容と市場データにもとづく整理として本メモを生成します。<br><br>
<a href="/consult/hearing?session_id={sid}">▶ ヒアリング入力へ進む</a>
</div>"#
    ));
    html.push_str("</div>\n</body>\n</html>\n");
    html
}

/// 個社別アクションメモ HTML を生成する (§14。顧客共有可)。
///
/// - `analysis`: 面談前の分析 (市場側の証拠・仮説)
/// - `answers`: 最新ヒアリング回答
/// - `reviews`: 仮説更新 (支持/否定/保留)
/// - `region` / `as_of`: 表示用
pub fn action_memo_html(
    analysis: &ConsultAnalysis,
    answers: &BTreeMap<String, AnswerValue>,
    reviews: &[HypothesisReview],
    region: &str,
    as_of: &str,
) -> String {
    let bottleneck = judge_bottleneck(answers);
    let actions = select_actions(analysis, reviews, &bottleneck);
    let goals = recruiting_goal(answers);
    let funnel = confirmed_funnel(answers);
    let rejected = rejected_hypotheses(analysis, reviews);
    let opens = open_items(answers, analysis, reviews);
    let supported = supported_hypotheses(analysis, reviews);

    let region_disp = if region.trim().is_empty() {
        "（未特定）".to_string()
    } else {
        escape_html(region)
    };
    let as_of_disp = if as_of.trim().is_empty() {
        String::new()
    } else {
        escape_html(as_of)
    };

    let mut html = String::with_capacity(48 * 1024);
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\" data-theme=\"default\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<meta name=\"robots\" content=\"noindex,nofollow\">\n");
    html.push_str("<title>個社別アクションメモ</title>\n<style>\n");
    html.push_str(&crate::handlers::survey::report_html::navy_css_bundle());
    html.push_str(memo_css());
    html.push_str("</style>\n</head>\n<body class=\"theme-navy\">\n");
    html.push_str("<div class=\"page-navy\">\n");

    // ヘッダ
    html.push_str(&format!(
        r#"<div class="page-head">
  <div class="ph-sec">個社別アクションメモ</div>
  <div class="ph-title">{region}</div>
  <div class="ph-sub">作成日: {as_of}</div>
  <div class="ph-rule"></div>
</div>"#,
        region = region_disp,
        as_of = as_of_disp
    ));

    // 位置づけ (§3)
    html.push_str(
        r#"<div class="memo-intro">
本メモは、<strong>お打ち合わせで伺った内容</strong>と、<strong>公開されている市場データ</strong>をもとに、
現在の採用活動の状況と次に取り組む候補を整理したものです。
断定ではなく、確認できた事実と、これから確かめる仮説を分けて記載しています。
記載の中で「お打ち合わせ内容」と「市場データ」は出典を区別して表記しています。
</div>"#,
    );

    // 1. 採用目標 (§14.2-1)
    html.push_str("<div class=\"memo-sec\">\n");
    html.push_str(&format!(
        "<h2><span class=\"memo-no\">1</span>採用目標 {}</h2>\n",
        source_tag_hearing()
    ));
    if goals.is_empty() {
        html.push_str("<p class=\"memo-hyp memo-hyp-note\">採用人数・期限・理由は、次回のお打ち合わせで整理します。</p>\n");
    } else {
        html.push_str("<table class=\"memo-kv\"><tbody>\n");
        for g in &goals {
            html.push_str(&format!(
                "<tr><th>{}</th><td>{}</td></tr>\n",
                escape_html(&g.label),
                escape_html(&g.value)
            ));
        }
        html.push_str("</tbody></table>\n");
    }
    html.push_str("</div>\n");

    // 2. 現在の主要ボトルネック (§14.2-2)
    html.push_str("<div class=\"memo-sec\">\n");
    html.push_str("<h2><span class=\"memo-no\">2</span>現在の主要ボトルネック</h2>\n");
    html.push_str(&format!(
        "<div class=\"memo-bottleneck\">{}</div>\n",
        escape_html(&bottleneck.note)
    ));
    html.push_str("</div>\n");

    // 3. 判断根拠 (§14.2-3) — 確認済みのファネル・運用 (ヒアリング) を提示
    html.push_str("<div class=\"memo-sec\">\n");
    html.push_str(&format!(
        "<h2><span class=\"memo-no\">3</span>判断根拠（確認できた事実） {}</h2>\n",
        source_tag_hearing()
    ));
    if funnel.is_empty() {
        html.push_str("<p class=\"memo-hyp memo-hyp-note\">応募〜承諾の各段の件数は、これから記録して確認していきます。</p>\n");
    } else {
        html.push_str("<table class=\"memo-kv\"><tbody>\n");
        for f in &funnel {
            html.push_str(&format!(
                "<tr><th>{}</th><td>{}</td></tr>\n",
                escape_html(&f.label),
                escape_html(&f.value)
            ));
        }
        html.push_str("</tbody></table>\n");
    }
    // 支持された仮説 (市場データ由来。未確認だが根拠あり) は別区分で
    if !supported.is_empty() {
        html.push_str(&format!(
            "<p style=\"font-size:9.5pt;margin-top:2.5mm;\"><strong>お打ち合わせで確からしいと確認できた見立て</strong> {}</p>\n",
            source_tag_market()
        ));
        html.push_str("<ul class=\"memo-list\">\n");
        for h in &supported {
            html.push_str(&format!(
                "<li>{}（検証の手がかり: {}）</li>\n",
                escape_html(&neutralize(&h.statement)),
                escape_html(&h.supporting_evidence_ids.join(", "))
            ));
        }
        html.push_str("</ul>\n");
    }
    html.push_str("</div>\n");

    // 4. 否定された仮説 (§14.2-4)
    html.push_str("<div class=\"memo-sec\">\n");
    html.push_str(
        "<h2><span class=\"memo-no\">4</span>今回は当てはまらないと確認できた見立て</h2>\n",
    );
    if rejected.is_empty() {
        html.push_str("<p class=\"memo-hyp memo-hyp-note\">今回のお打ち合わせでは、明確に否定された見立てはありませんでした。</p>\n");
    } else {
        html.push_str("<ul class=\"memo-list\">\n");
        for (h, note) in &rejected {
            let note_disp = if note.trim().is_empty() {
                String::new()
            } else {
                format!("（理由: {}）", escape_html(note))
            };
            html.push_str(&format!(
                "<li>{}{}</li>\n",
                escape_html(&neutralize(&h.statement)),
                note_disp
            ));
        }
        html.push_str("</ul>\n");
    }
    html.push_str("</div>\n");

    // 5. 未確認事項 (§14.2-5)
    html.push_str("<div class=\"memo-sec\">\n");
    html.push_str("<h2><span class=\"memo-no\">5</span>これから確認する事項</h2>\n");
    if opens.is_empty() {
        html.push_str(
            "<p class=\"memo-hyp memo-hyp-note\">現時点で追加の確認事項はありません。</p>\n",
        );
    } else {
        html.push_str("<ul class=\"memo-list\">\n");
        for o in &opens {
            html.push_str(&format!("<li>{}</li>\n", escape_html(&neutralize(o))));
        }
        html.push_str("</ul>\n");
    }
    html.push_str("</div>\n");

    // 6+7+8. 優先施策 + KPI + 担当・期限 (§14.2-6,7,8 / §14.4 表形式)
    html.push_str("<div class=\"memo-sec\">\n");
    html.push_str("<h2><span class=\"memo-no\">6</span>優先して取り組む候補</h2>\n");
    html.push_str("<p style=\"font-size:9pt;color:#6A6E7A;margin:0 0 2mm;\">担当・期限・判定日は記入欄です。お打ち合わせで決めた内容を記入してください。</p>\n");
    if actions.is_empty() {
        html.push_str("<p class=\"memo-hyp memo-hyp-note\">現時点で提示できる施策候補がありません。各段の件数を記録するところから始めます。</p>\n");
    } else {
        html.push_str("<table class=\"memo-actions\"><thead><tr>");
        html.push_str("<th style=\"width:8mm\">優先度</th><th>施策</th><th style=\"width:22mm\">分類</th><th style=\"width:20mm\">根拠</th><th style=\"width:18mm\">担当</th><th style=\"width:20mm\">期限</th><th style=\"width:28mm\">KPI</th><th style=\"width:18mm\">判定日</th>");
        html.push_str("</tr></thead><tbody>\n");
        for a in &actions {
            html.push_str(&format!(
                "<tr><td class=\"{cls}\">{prio}</td><td>{act}</td><td>{cat}</td><td>{refs}</td><td class=\"memo-fill\"></td><td class=\"memo-fill\"></td><td>{kpi}</td><td class=\"memo-fill\"></td></tr>\n",
                cls = priority_class(a.priority),
                prio = a.priority.label_ja(),
                act = escape_html(&a.action),
                cat = escape_html(a.category.label_ja()),
                refs = escape_html(&a.evidence_refs.join(", ")),
                kpi = escape_html(&a.kpi),
            ));
        }
        html.push_str("</tbody></table>\n");
    }
    html.push_str("</div>\n");

    // 7. 次回レビュー日 (§14.2-9 相当。KPI/担当・期限は 6 の表に統合したため表示連番は 7)
    html.push_str("<div class=\"memo-sec\">\n");
    html.push_str("<h2><span class=\"memo-no\">7</span>次回レビュー日</h2>\n");
    html.push_str(
        "<div class=\"memo-review-fill\" contenteditable=\"true\">　　　年　　月　　日</div>\n",
    );
    html.push_str("</div>\n");

    html.push_str("</div>\n</body>\n</html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::evidence_pack::analyze;
    use crate::handlers::consult::evidence_pack::tests::rich_input;
    use crate::handlers::consult::hypothesis_review::{Decision, HypothesisReview};

    fn av(value: &str) -> AnswerValue {
        AnswerValue {
            value: value.to_string(),
            unknown: false,
            no_data: false,
        }
    }
    fn av_unknown() -> AnswerValue {
        AnswerValue {
            value: String::new(),
            unknown: true,
            no_data: false,
        }
    }
    fn av_nodata() -> AnswerValue {
        AnswerValue {
            value: String::new(),
            unknown: false,
            no_data: true,
        }
    }

    fn answers(pairs: &[(&str, AnswerValue)]) -> BTreeMap<String, AnswerValue> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    // ---- ボトルネック判定 ----

    #[test]
    fn bottleneck_empty_funnel_needs_measurement() {
        let a = answers(&[("q13_biggest_challenge", av("応募が来ない"))]);
        let b = judge_bottleneck(&a);
        assert!(b.stage.is_none());
        assert!(b.measurement_needed);
        assert!(b.note.contains("記録"));
    }

    #[test]
    fn bottleneck_application_when_few_applications_only() {
        let a = answers(&[("q04_applications_monthly", av("0〜2件"))]);
        let b = judge_bottleneck(&a);
        assert_eq!(b.stage, Some(FunnelStage::Application));
    }

    #[test]
    fn bottleneck_detects_narrowest_stage_contact() {
        // 応募多い(代表5) → 接触2 (通過率0.4) → 面接設定2 (1.0) → 実施2 → 内定2 → 承諾2
        // 接触段の通過率が最も低い → 接触がボトルネック
        let a = answers(&[
            ("q04_applications_monthly", av("3件以上")),
            ("q05_contacts", av("2")),
            ("q06_interviews_set", av("2")),
            ("q07_interviews_done", av("2")),
            ("q08_offers", av("2")),
            ("q09_acceptances", av("2")),
        ]);
        let b = judge_bottleneck(&a);
        assert_eq!(b.stage, Some(FunnelStage::Contact));
        assert!(!b.measurement_needed);
    }

    #[test]
    fn bottleneck_detects_acceptance_when_offers_high_acceptance_low() {
        // 各段そこそこ通過し、内定10→承諾1 (通過率0.1) が最細 → 承諾
        let a = answers(&[
            ("q04_applications_monthly", av("3件以上")),
            ("q05_contacts", av("5")),
            ("q06_interviews_set", av("5")),
            ("q07_interviews_done", av("5")),
            ("q08_offers", av("10")),
            ("q09_acceptances", av("1")),
        ]);
        let b = judge_bottleneck(&a);
        assert_eq!(b.stage, Some(FunnelStage::Acceptance));
    }

    #[test]
    fn bottleneck_unknown_stage_switches_to_measurement() {
        // 接触が不明 → any_unknown → measurement_needed
        let a = answers(&[
            ("q04_applications_monthly", av("3件以上")),
            ("q05_contacts", av_unknown()),
            ("q06_interviews_set", av("3")),
        ]);
        let b = judge_bottleneck(&a);
        assert!(b.measurement_needed, "不明の段があれば計測整備を提案");
        assert!(b.note.contains("計測"));
    }

    #[test]
    fn bottleneck_no_data_stage_flags_measurement() {
        let a = answers(&[
            ("q04_applications_monthly", av("3件以上")),
            ("q05_contacts", av("5")),
            ("q06_interviews_set", av_nodata()),
        ]);
        let b = judge_bottleneck(&a);
        assert!(b.measurement_needed);
    }

    // ---- 施策選定 ----

    fn review(id: &str, decision: Decision) -> HypothesisReview {
        HypothesisReview {
            hypothesis_id: id.to_string(),
            auto_suggestion: decision,
            decision,
            note: String::new(),
        }
    }

    #[test]
    fn selected_actions_always_have_evidence_refs() {
        let analysis = analyze(&rich_input());
        // 支持: TOP仮説を支持に
        let reviews: Vec<HypothesisReview> = analysis
            .top_hypotheses
            .iter()
            .map(|h| review(&h.hypothesis_id, Decision::Support))
            .collect();
        let a = answers(&[
            ("q04_applications_monthly", av("3件以上")),
            ("q05_contacts", av("2")),
            ("q06_interviews_set", av("2")),
        ]);
        let b = judge_bottleneck(&a);
        let actions = select_actions(&analysis, &reviews, &b);
        assert!(!actions.is_empty(), "施策が選定される");
        assert!(actions.len() <= config::ACTION_MEMO_MAX_ACTIONS);
        for act in &actions {
            assert!(
                !act.evidence_refs.is_empty(),
                "全施策に根拠IDが必要 (§14.4): {}",
                act.action
            );
        }
    }

    #[test]
    fn measurement_action_selected_when_no_funnel() {
        let analysis = analyze(&rich_input());
        let a = answers(&[("q13_biggest_challenge", av("わからない"))]);
        let b = judge_bottleneck(&a);
        let actions = select_actions(&analysis, &[], &b);
        assert!(
            actions
                .iter()
                .any(|x| x.category == ActionCategory::Measurement),
            "ファネル不明時はデータ計測改善が選ばれる"
        );
    }

    // ---- メモ HTML ----

    #[test]
    fn memo_html_has_nine_sections_and_no_internal_band() {
        let analysis = analyze(&rich_input());
        let reviews = vec![review(
            analysis.top_hypotheses[0].hypothesis_id.as_str(),
            Decision::Support,
        )];
        let a = answers(&[
            ("q01_hiring_count", av("3")),
            ("q02_deadline", av("2026年9月末")),
            ("q04_applications_monthly", av("3件以上")),
            ("q05_contacts", av("2")),
            ("q06_interviews_set", av("2")),
            ("q07_interviews_done", av("2")),
            ("q08_offers", av("1")),
            ("q09_acceptances", av("1")),
        ]);
        let html = action_memo_html(&analysis, &a, &reviews, "群馬県 高崎市", "2026-07-11");

        // 9セクション見出し
        for h in [
            "採用目標",
            "現在の主要ボトルネック",
            "判断根拠",
            "当てはまらないと確認できた見立て",
            "これから確認する事項",
            "優先して取り組む候補",
            "次回レビュー日",
        ] {
            assert!(html.contains(h), "セクション {} がメモにない", h);
        }
        // 顧客共有可: 社内用帯を付けない (§3)
        assert!(!html.contains("社内用"), "顧客共有メモに社内用帯は付けない");
        assert!(!html.contains("顧客配布不可"));
        // 冒頭に位置づけ
        assert!(html.contains("お打ち合わせで伺った内容"));
        // 出典区別 (§24-9)
        assert!(html.contains("お打ち合わせ内容"));
        assert!(html.contains("市場データ"));
    }

    #[test]
    fn memo_html_has_no_evaluative_or_forbidden_words() {
        // 中立表現違反 (「劣位」「縮小」「集中」等) や §19.2 禁止表現がメモに無いこと
        let analysis = analyze(&rich_input());
        let reviews: Vec<HypothesisReview> = analysis
            .top_hypotheses
            .iter()
            .map(|h| review(&h.hypothesis_id, Decision::Support))
            .collect();
        let a = answers(&[
            ("q04_applications_monthly", av("0〜2件")),
            ("q05_contacts", av_unknown()),
        ]);
        let html = action_memo_html(&analysis, &a, &reviews, "群馬県", "2026-07-11");
        // <style> ブロック (共有 navy CSS。コメントに「縮小」等を含む) を除いた本文を検査対象にする
        let body = strip_style_blocks(&html);
        for banned in [
            "劣位",
            "優位",
            "縮小",
            "集中",
            "劇的",
            "完璧",
            "必ず採用できる",
            "応募が増える",
            "離職率が高い企業",
            "成長企業である",
            "この媒体が最適",
            "SalesNow",
            "ブリーフ",
        ] {
            assert!(
                !body.contains(banned),
                "顧客向けメモ本文に不適切表現 {} が含まれる",
                banned
            );
        }
    }

    /// `<style>...</style>` ブロックを除去して本文だけを返す (テスト用)。
    fn strip_style_blocks(html: &str) -> String {
        let mut out = String::with_capacity(html.len());
        let mut rest = html;
        while let Some(start) = rest.find("<style") {
            out.push_str(&rest[..start]);
            if let Some(end_rel) = rest[start..].find("</style>") {
                rest = &rest[start + end_rel + "</style>".len()..];
            } else {
                rest = "";
                break;
            }
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn neutralize_removes_evaluative_words() {
        for (input, banned) in [
            ("給与面の優位があっても掲載が長引く", "優位"),
            ("条件比較の土俵で埋もれている可能性", "埋もれ"),
            ("地域の応募母集団を吸収している可能性", "吸収"),
            ("特定企業の大量募集が続く", "大量募集"),
        ] {
            let out = neutralize(input);
            assert!(!out.contains(banned), "「{}」が残っている: {}", banned, out);
        }
    }

    #[test]
    fn memo_html_shows_rejected_and_open_items() {
        let analysis = analyze(&rich_input());
        // 1件否定 + 1件保留
        let ids: Vec<String> = analysis
            .hypotheses
            .iter()
            .map(|h| h.hypothesis_id.clone())
            .collect();
        let mut reviews = vec![HypothesisReview {
            hypothesis_id: ids[0].clone(),
            auto_suggestion: Decision::Hold,
            decision: Decision::Reject,
            note: "自社では該当しないと確認".to_string(),
        }];
        if ids.len() > 1 {
            reviews.push(review(&ids[1], Decision::Hold));
        }
        let a = answers(&[
            ("q04_applications_monthly", av("3件以上")),
            ("q05_contacts", av("5")),
            ("q08_offers", av_nodata()),
        ]);
        let html = action_memo_html(&analysis, &a, &reviews, "群馬県", "2026-07-11");
        // 否定理由が出る
        assert!(html.contains("自社では該当しないと確認"));
        // データなしだった項目が「これから確認する事項」に出る
        assert!(html.contains("データなし"));
    }
}
