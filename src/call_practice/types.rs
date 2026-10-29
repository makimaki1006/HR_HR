//! 架電の練習の API の型。React 側へ ts-rs で書き出す (画面と API の契約)。
//! 隠れた答え・型の名前は、練習を終えるまでブラウザへ送る型に入れない。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::crm::talk_review::types::TalkTurn;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PracticeConfig {
    pub enabled: bool,
    /// 使えないときの理由 (画面にそのまま出せる日本語)
    pub unavailable_reason: Option<String>,
    pub max_turns: u32,
    pub max_minutes: u32,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
pub struct PracticeStartRequest {
    /// 型の指定 (管理者の確認用と「同じ型でもう一度」)。省略はおまかせ
    #[serde(default)]
    pub persona_id: Option<String>,
    /// 台本を横に出して練習するか
    #[serde(default)]
    pub script_seen: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PracticeStartResponse {
    pub session_id: String,
    pub opening: TalkTurn,
    pub max_turns: u32,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct PracticeTurnRequest {
    pub client_turn_id: String,
    pub text: String,
    /// "text" | "voice"
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PracticeTurnResponse {
    pub rep_turn: TalkTurn,
    pub customer_turn: TalkTurn,
    /// お客さん役が会話を閉じた、または往復の上限に来た
    pub ended: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PracticeSessionView {
    pub session_id: String,
    pub transcript: Vec<TalkTurn>,
    pub ended: bool,
    pub finished: bool,
    pub max_turns: u32,
    pub script_seen: bool,
    pub result: Option<PracticeResult>,
}

/// 記録欄の 1 欄。mode: "value" (値あり) | "not_heard" (聞けていない) | "empty" (空欄)
#[derive(Debug, Clone, Deserialize, TS)]
pub struct RecordField {
    pub mode: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct HearingRecord {
    pub hiring_3m: RecordField,
    pub hiring_later: RecordField,
    pub issue: RecordField,
    pub urgency: RecordField,
    pub consider: RecordField,
    pub budget: RecordField,
    pub decider: RecordField,
    pub attend: RecordField,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct PracticeFinishRequest {
    pub record: HearingRecord,
    /// "hot" | "mixed" | "cold"
    pub heat_guess: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PracticeItemResult {
    /// "hiring" | "issue" | "cost" | "decider"
    pub item: String,
    pub label: String,
    /// "got" | "wrong_record" | "not_heard" | "empty" | "recorded_without_answer"
    pub status: String,
    pub status_label: String,
    pub your_record: String,
    pub truth: String,
    /// 聞いた発言の番号
    pub turns: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct IrritationScene {
    pub turn: u32,
    pub customer_text: String,
    pub prev_rep_turn: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TurnComment {
    pub turn: u32,
    pub text: String,
    pub comment: String,
    pub better: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Commentary {
    pub summary: Option<String>,
    pub good: Vec<TurnComment>,
    pub bad: Vec<TurnComment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct QuestionBandRow {
    pub band: String,
    pub rate_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct HeatResult {
    pub guess: String,
    pub truth: String,
    pub hot_marks_hidden: u32,
    pub hot_marks_heard: u32,
    /// 聞き出せた印だけで決めたときの熱さ
    pub implied_from_heard: String,
    pub guess_matches_truth: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct HiddenAnswerRow {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PersonaReveal {
    pub id: String,
    pub label: String,
    pub reference: String,
    pub setting: String,
    pub hidden: Vec<HiddenAnswerRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PracticeResult {
    pub items: Vec<PracticeItemResult>,
    /// 「聞けていない」(または空欄) が残った項目の数
    pub not_heard_left: u32,
    pub not_heard_items: Vec<String>,
    pub question_count: u32,
    pub question_band: String,
    pub question_bands: Vec<QuestionBandRow>,
    pub shortest_note: String,
    pub monthly_fee_told: bool,
    pub asked_price: bool,
    pub irritations: Vec<IrritationScene>,
    pub heat: HeatResult,
    pub persona: PersonaReveal,
    pub commentary: Option<Commentary>,
    /// コメントを付けられなかったときの理由
    pub commentary_note: Option<String>,
    pub script_seen: bool,
    pub transcript: Vec<TalkTurn>,
    pub reference_note: String,
}
