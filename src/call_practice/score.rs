//! 採点。規則の部分 (記録と隠れた答えの照らし合わせ・質問の数・見立て) と、
//! MiniMax の部分 (良い例・悪い例のコメント、課題の趣旨の照らし合わせ)。練習を終えたとき 1 回だけ呼ぶ。

use serde_json::Value;

use super::persona::{self, Persona, SLOTS};
use super::types::*;
use crate::crm::talk_review::judge::{Completer, JudgeError};
use crate::crm::talk_review::lang::find_foreign;
use crate::crm::talk_review::types::TalkTurn;

/// 営業の発言 1 つぶんの印 (お客さん役の呼び出しで付いたものと規則)
#[derive(Debug, Clone, PartialEq)]
pub struct RepMark {
    pub turn: u32,
    pub slots: Vec<String>,
    pub is_question: bool,
    pub asked_price: bool,
}

pub struct ScoreInput<'a> {
    pub persona: &'static Persona,
    pub transcript: &'a [TalkTurn],
    pub marks: &'a [RepMark],
    /// 嫌がったお客さん役の発言の番号
    pub irritated_turns: &'a [u32],
    pub question_count: u32,
    pub fee_told: bool,
    /// 予算を聞いた時点の本当の答え (聞いていなければ None)
    pub budget_at_ask: Option<String>,
    pub script_seen: bool,
}

/// (項目, 表示名, [(欄, 必須か)])
type ItemDef = (&'static str, &'static str, &'static [(&'static str, bool)]);
const ITEMS: [ItemDef; 4] = [
    (
        "hiring",
        "① 時期と人数",
        &[("hiring_3m", true), ("hiring_later", false)],
    ),
    (
        "issue",
        "② 課題と意欲",
        &[("issue", true), ("urgency", true)],
    ),
    (
        "cost",
        "③ 検討と予算",
        &[("consider", true), ("budget", true)],
    ),
    (
        "decider",
        "④ 決める人と同席",
        &[("decider", true), ("attend", true)],
    ),
];

pub const BANDS: [(&str, &str); 4] = [
    ("1-4", "8.1% (31 / 382)"),
    ("5-7", "6.0% (47 / 783)"),
    ("8-10", "3.9% (38 / 987)"),
    ("11+", "4.0% (21 / 528)"),
];

pub fn question_band(n: u32) -> &'static str {
    match n {
        0..=4 => "1-4",
        5..=7 => "5-7",
        8..=10 => "8-10",
        _ => "11+",
    }
}

fn band_label(b: &str) -> String {
    match b {
        "1-4" => "1〜4 問".into(),
        "5-7" => "5〜7 問".into(),
        "8-10" => "8〜10 問".into(),
        _ => "11 問以上".into(),
    }
}

fn field_label(slot: &str) -> &'static str {
    match slot {
        "hiring_3m" => "3か月以内の職種と人数",
        "hiring_later" => "それ以外の募集",
        "issue" => "課題",
        "urgency" => "解決の早さ",
        "consider" => "検討",
        "budget" => "予算",
        "decider" => "決める人",
        _ => "同席",
    }
}

/// 全角数字・漢数字・空白をそろえて比べやすくする
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '　')
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c),
            '一' => '1',
            '二' => '2',
            '三' => '3',
            '四' => '4',
            '五' => '5',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

fn text_matches(groups: &[Vec<String>], record: &str) -> bool {
    let r = normalize(record);
    groups
        .iter()
        .all(|alts| alts.iter().any(|a| r.contains(&normalize(a))))
}

fn select_label(slot: &str, v: &str) -> String {
    match slot {
        "urgency" => persona::urgency_label(v),
        "consider" => persona::consider_label(v),
        "budget" => persona::budget_label(v),
        "decider" => persona::decider_label(v),
        "attend" => persona::attend_label(v),
        _ => return v.to_string(),
    }
    .to_string()
}

fn record_field<'a>(r: &'a HearingRecord, slot: &str) -> &'a RecordField {
    match slot {
        "hiring_3m" => &r.hiring_3m,
        "hiring_later" => &r.hiring_later,
        "issue" => &r.issue,
        "urgency" => &r.urgency,
        "consider" => &r.consider,
        "budget" => &r.budget,
        "decider" => &r.decider,
        _ => &r.attend,
    }
}

fn record_display(slot: &str, f: &RecordField) -> String {
    match f.mode.as_str() {
        "not_heard" => "聞けていない".into(),
        "value" if !f.value.trim().is_empty() => match slot {
            "hiring_3m" | "hiring_later" | "issue" => f.value.trim().to_string(),
            _ => select_label(slot, f.value.trim()),
        },
        _ => "空欄".into(),
    }
}

fn truth_display(p: &Persona, slot: &str, budget_truth: &str) -> String {
    if let Some(t) = p.text_answer(slot) {
        return t.truth.clone();
    }
    if slot == "budget" && p.hidden.budget.value == "no_then_yes" {
        return format!(
            "{} (最初は「無い」と言い、月額定額を聞いたあとは「ある」に変わる)",
            persona::budget_label(budget_truth)
        );
    }
    let v = p.choice(slot).map(|c| c.value.as_str()).unwrap_or("");
    select_label(slot, v)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FieldState {
    Got,
    Wrong,
    NoAnswerValue,
    Empty,
    NotHeard,
}

fn field_state(
    p: &Persona,
    slot: &str,
    f: &RecordField,
    answered: bool,
    budget_truth: &str,
    issue_match: Option<bool>,
) -> FieldState {
    let has_value = f.mode == "value" && !f.value.trim().is_empty();
    if has_value {
        if !answered {
            return FieldState::NoAnswerValue;
        }
        let ok = match p.text_answer(slot) {
            Some(t) => {
                if slot == "issue" {
                    issue_match.unwrap_or_else(|| text_matches(&t.matches, &f.value))
                } else {
                    text_matches(&t.matches, &f.value)
                }
            }
            None => {
                let truth = if slot == "budget" {
                    budget_truth.to_string()
                } else {
                    p.choice(slot).map(|c| c.value.clone()).unwrap_or_default()
                };
                f.value.trim() == truth
            }
        };
        return if ok {
            FieldState::Got
        } else {
            FieldState::Wrong
        };
    }
    if answered {
        return FieldState::Wrong;
    }
    if f.mode == "not_heard" {
        FieldState::NotHeard
    } else {
        FieldState::Empty
    }
}

fn status_label(s: &str) -> &'static str {
    match s {
        "got" => "取れた",
        "wrong_record" => "記録が違う",
        "not_heard" => "聞けていないと記録",
        "empty" => "空欄",
        _ => "聞かずに記入",
    }
}

pub fn build_result(
    inp: &ScoreInput<'_>,
    req: &PracticeFinishRequest,
    commentary: Option<Commentary>,
    commentary_note: Option<String>,
    issue_match: Option<bool>,
) -> PracticeResult {
    let p = inp.persona;
    let asked_all: Vec<&str> = inp
        .marks
        .iter()
        .flat_map(|m| m.slots.iter().map(String::as_str))
        .collect();
    let answered = |slot: &str| asked_all.contains(&slot);
    let budget_truth = inp
        .budget_at_ask
        .clone()
        .unwrap_or_else(|| p.budget_final(inp.fee_told));

    let mut items = vec![];
    for (id, label, fields) in ITEMS {
        let mut states = vec![];
        let mut rec_parts = vec![];
        let mut truth_parts = vec![];
        for (slot, required) in fields {
            let f = record_field(&req.record, slot);
            let ans = answered(slot);
            let st = field_state(p, slot, f, ans, &budget_truth, issue_match);
            let counts = *required || ans || st == FieldState::NoAnswerValue;
            if counts {
                states.push(st);
            }
            rec_parts.push(format!(
                "{}: {}",
                field_label(slot),
                record_display(slot, f)
            ));
            truth_parts.push(format!(
                "{}: {}",
                field_label(slot),
                truth_display(p, slot, &budget_truth)
            ));
        }
        let status = if states.contains(&FieldState::Wrong) {
            "wrong_record"
        } else if states.contains(&FieldState::NoAnswerValue) {
            "recorded_without_answer"
        } else if states.iter().all(|s| *s == FieldState::Got) {
            "got"
        } else if states.contains(&FieldState::Empty) {
            "empty"
        } else {
            "not_heard"
        };
        let mut turns: Vec<u32> = inp
            .marks
            .iter()
            .filter(|m| fields.iter().any(|(s, _)| m.slots.iter().any(|x| x == s)))
            .map(|m| m.turn)
            .collect();
        turns.dedup();
        items.push(PracticeItemResult {
            item: id.to_string(),
            label: label.to_string(),
            status: status.to_string(),
            status_label: status_label(status).to_string(),
            your_record: rec_parts.join(" / "),
            truth: truth_parts.join(" / "),
            turns,
        });
    }
    let not_heard_items: Vec<String> = items
        .iter()
        .filter(|i| i.status == "not_heard" || i.status == "empty")
        .map(|i| i.label.clone())
        .collect();

    // 嫌がられた場面
    let irritations: Vec<IrritationScene> = inp
        .irritated_turns
        .iter()
        .filter_map(|t| {
            let c = inp.transcript.iter().find(|x| x.n == *t)?;
            let prev = inp
                .transcript
                .iter()
                .rev()
                .find(|x| x.n < *t && x.speaker == "rep")
                .map(|x| x.n);
            Some(IrritationScene {
                turn: *t,
                customer_text: c.text.clone(),
                prev_rep_turn: prev,
            })
        })
        .collect();

    // 見立て
    let hidden_marks = p.hot_marks();
    let heard_marks = hidden_marks
        .iter()
        .filter(|m| {
            answered(m)
                && (**m != "budget"
                    || p.hidden.budget.value != "no_then_yes"
                    || budget_truth == "yes")
        })
        .count() as u32;
    let truth_heat = persona::heat_from_marks(hidden_marks.len() as u32);
    let implied = persona::heat_from_marks(heard_marks);
    let guess = req.heat_guess.as_str();
    let guess = if ["hot", "mixed", "cold"].contains(&guess) {
        guess
    } else {
        "mixed"
    };
    let matches = guess == truth_heat;
    let message = if matches && implied == truth_heat {
        "見立ては当たりです。聞き出せた答えからも同じ判断ができます。".to_string()
    } else if matches {
        format!(
            "見立ては当たりですが、聞き出せた答えからは「{}」が妥当でした。隠れた答えまで聞けていれば、根拠をもって判断できました。",
            persona::heat_label(implied)
        )
    } else if guess == implied {
        format!(
            "見立ては外れましたが、聞き出せた答えからは「{}」が妥当でした。本当の型は「{}」で、聞けていない項目に熱さの印がありました。",
            persona::heat_label(guess),
            persona::heat_label(truth_heat)
        )
    } else {
        format!(
            "見立ては外れでした。本当の型は「{}」です。",
            persona::heat_label(truth_heat)
        )
    };

    let hidden_rows: Vec<HiddenAnswerRow> = SLOTS
        .iter()
        .map(|s| HiddenAnswerRow {
            label: field_label(s).to_string(),
            value: truth_display(p, s, &budget_truth),
        })
        .collect();

    let band = question_band(inp.question_count).to_string();
    PracticeResult {
        not_heard_left: not_heard_items.len() as u32,
        not_heard_items,
        items,
        question_count: inp.question_count,
        question_band: band,
        question_bands: BANDS
            .iter()
            .map(|(b, r)| QuestionBandRow {
                band: band_label(b),
                rate_text: r.to_string(),
            })
            .collect(),
        shortest_note: "4 項目を聞く最短は 6〜8 問が目安です。".into(),
        monthly_fee_told: inp.fee_told,
        asked_price: inp.marks.iter().any(|m| m.asked_price),
        irritations,
        heat: HeatResult {
            guess: guess.to_string(),
            truth: truth_heat.to_string(),
            hot_marks_hidden: hidden_marks.len() as u32,
            hot_marks_heard: heard_marks,
            implied_from_heard: implied.to_string(),
            guess_matches_truth: matches,
            message,
        },
        persona: PersonaReveal {
            id: p.id.clone(),
            label: p.label.clone(),
            reference: p.reference.clone(),
            setting: p.setting_text(),
            hidden: hidden_rows,
        },
        commentary,
        commentary_note,
        script_seen: inp.script_seen,
        transcript: inp.transcript.to_vec(),
        reference_note: "成約率は相関の値です。聞き方を変えたときの成約の増え方は示していません。人・時期をそろえていない素の率です。".into(),
    }
}

// ---------------------------------------------------------------------------
// MiniMax の部分
// ---------------------------------------------------------------------------

const BANNED: [&str; 3] = ["確実に", "必ず", "100%"];
const MAX_EXAMPLES: usize = 3;
const MAX_COMMENT_CHARS: usize = 120;

pub fn build_prompt(inp: &ScoreInput<'_>, req: &PracticeFinishRequest) -> String {
    let tr: Vec<String> = inp
        .transcript
        .iter()
        .map(|t| {
            format!(
                "{}. {}: {}",
                t.n,
                if t.speaker == "rep" { "営" } else { "先" },
                t.text
            )
        })
        .collect();
    let rec_issue = record_display("issue", &req.record.issue);
    format!(
        r#"あなたは新規営業のヒアリング練習を振り返る担当です。下の文字起こし(営=営業の練習をしている人、先=お客さん役。番号は発言の順番)を読み、営業の発言の良かった点と直すとよい点を挙げてください。

決まり:
- 番号は営の発言のものだけを使う。良い例・悪い例は各0〜3個。
- 台本の言い回しの例: 「当日のご提案を御社に合わせたいので、少しだけ伺ってもよろしいですか」「直近3か月以内で、採用が必要な職種はありますか」「今の採用で困っていることと、どのくらい早く解決したいかを教えていただけますか」「弊社は月額定額のサービスです。その形でしたら、ご検討いただけそうですか」「最終的にお決めになる方と、当日ご同席いただけるかを伺えますか」
- 金額を直接聞く、「差し支えなければ」を付ける、前置きなしにいきなり質問を始める、は直すとよい点にする。
- 直すとよい点には、台本の言い回しを参考にした言い直し(better)を添える。
- 平易な日本語で書く。「必ず」「確実に」「100%」は使わない。
- issue_match: お客さん役の本当の課題「{truth}」と、営業の記録した課題「{rec}」が同じ趣旨なら true、違えば false、記録が空欄か「聞けていない」なら null。

文字起こし:
{tr}

次の JSON だけを返してください:
{{"summary": "この練習で一番直すとよい点を1文で",
 "good": [{{"turn": 番号, "comment": "..."}}],
 "bad": [{{"turn": 番号, "comment": "...", "better": "..."}}],
 "issue_match": true/false/null}}"#,
        truth = inp.persona.hidden.issue.truth,
        rec = rec_issue,
        tr = tr.join("\n"),
    )
}

fn clean_text(s: Option<&str>, source: &str) -> Option<String> {
    let s = s?.trim();
    if s.is_empty()
        || s.chars().count() > MAX_COMMENT_CHARS
        || BANNED.iter().any(|b| s.contains(b))
        || find_foreign(s, source).is_some()
    {
        return None;
    }
    Some(s.to_string())
}

/// MiniMax の応答 → コメントと課題の趣旨。営業の発言の番号にないものは捨てる
pub fn parse_commentary(v: &Value, transcript: &[TalkTurn]) -> (Commentary, Option<bool>) {
    let source: String = transcript
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let rep_text = |n: u32| {
        transcript
            .iter()
            .find(|t| t.n == n && t.speaker == "rep")
            .map(|t| t.text.clone())
    };
    let same_words = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_whitespace() && !"。、？?！!「」".contains(*c))
            .collect()
    };
    let collect = |key: &str, with_better: bool| -> Vec<TurnComment> {
        v.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        let n = x
                            .get("turn")
                            .and_then(|t| t.as_u64().or_else(|| t.as_str()?.parse().ok()))?
                            as u32;
                        let text = rep_text(n)?;
                        let comment =
                            clean_text(x.get("comment").and_then(Value::as_str), &source)?;
                        let better = if with_better {
                            // 元の発言と同じ言い直しは助言にならないので出さない
                            clean_text(x.get("better").and_then(Value::as_str), &source)
                                .filter(|b| same_words(b) != same_words(&text))
                        } else {
                            None
                        };
                        Some(TurnComment {
                            turn: n,
                            text,
                            comment,
                            better,
                        })
                    })
                    .take(MAX_EXAMPLES)
                    .collect()
            })
            .unwrap_or_default()
    };
    let issue_match = v.get("issue_match").and_then(Value::as_bool);
    (
        Commentary {
            summary: clean_text(v.get("summary").and_then(Value::as_str), &source),
            good: collect("good", false),
            bad: collect("bad", true),
        },
        issue_match,
    )
}

fn has_foreign(v: &Value, transcript: &[TalkTurn]) -> bool {
    let source: String = transcript
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut texts: Vec<&str> = vec![];
    if let Some(s) = v.get("summary").and_then(Value::as_str) {
        texts.push(s);
    }
    for key in ["good", "bad"] {
        for x in v.get(key).and_then(Value::as_array).into_iter().flatten() {
            for k in ["comment", "better"] {
                if let Some(s) = x.get(k).and_then(Value::as_str) {
                    texts.push(s);
                }
            }
        }
    }
    texts.iter().any(|t| find_foreign(t, &source).is_some())
}

const STRICT_SUFFIX: &str =
    "\n\n重要: 文章はすべて日本語だけで書く。中国語・韓国語・英語の単語を混ぜない。";

/// 日本語以外が混ざったら 1 回だけ書き直させる。通信の失敗は Err
pub async fn commentary_via<C: Completer>(
    c: &C,
    inp: &ScoreInput<'_>,
    req: &PracticeFinishRequest,
) -> Result<(Commentary, Option<bool>), JudgeError> {
    let base = build_prompt(inp, req);
    let mut v = c.complete(&base).await?;
    if has_foreign(&v, inp.transcript) {
        if let Ok(v2) = c.complete(&format!("{base}{STRICT_SUFFIX}")).await {
            v = v2;
        }
    }
    Ok(parse_commentary(&v, inp.transcript))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_practice::persona::find;

    fn f(mode: &str, value: &str) -> RecordField {
        RecordField {
            mode: mode.into(),
            value: value.into(),
        }
    }

    fn rec(h3: RecordField, hl: RecordField, issue: RecordField) -> HearingRecord {
        HearingRecord {
            hiring_3m: h3,
            hiring_later: hl,
            issue,
            urgency: f("value", "early"),
            consider: f("value", "positive"),
            budget: f("value", "yes"),
            decider: f("value", "boss"),
            attend: f("value", "yes"),
        }
    }

    fn turns() -> Vec<TalkTurn> {
        let mk = |n, s: &str, t: &str| TalkTurn {
            n,
            speaker: s.into(),
            text: t.into(),
        };
        vec![
            mk(1, "customer", "はい、ではその日でお願いします"),
            mk(2, "rep", "直近3か月以内で、採用が必要な職種はありますか？"),
            mk(3, "customer", "ドライバーを2名ですね"),
            mk(4, "rep", "予算はいくらですか？"),
            mk(5, "customer", "それ、当日じゃだめですか"),
        ]
    }

    fn mark(turn: u32, slots: &[&str]) -> RepMark {
        RepMark {
            turn,
            slots: slots.iter().map(|s| s.to_string()).collect(),
            is_question: true,
            asked_price: false,
        }
    }

    fn input<'a>(
        tr: &'a [TalkTurn],
        marks: &'a [RepMark],
        irr: &'a [u32],
        q: u32,
    ) -> ScoreInput<'a> {
        ScoreInput {
            persona: find("hot_01").unwrap(),
            transcript: tr,
            marks,
            irritated_turns: irr,
            question_count: q,
            fee_told: false,
            budget_at_ask: None,
            script_seen: true,
        }
    }

    #[test]
    fn 項目ごとの区分を隠れた答えとの照らし合わせで決める() {
        let tr = turns();
        let marks = vec![mark(2, &["hiring_3m"]), mark(4, &["budget"])];
        let irr = [5u32];
        let req = PracticeFinishRequest {
            record: rec(
                f("value", "ドライバー　２名"),
                f("not_heard", ""),
                f("value", "前任が辞めた"),
            ),
            heat_guess: "hot".into(),
        };
        let r = build_result(&input(&tr, &marks, &irr, 2), &req, None, None, None);
        let st: Vec<(&str, &str)> = r
            .items
            .iter()
            .map(|i| (i.item.as_str(), i.status.as_str()))
            .collect();
        // ① 全角数字でも合う。② 聞いていないのに記入。③ 予算だけ聞いた (検討は聞いていない) のに両方記入。④ 聞いていないのに記入
        assert_eq!(
            st,
            [
                ("hiring", "got"),
                ("issue", "recorded_without_answer"),
                ("cost", "recorded_without_answer"),
                ("decider", "recorded_without_answer"),
            ]
        );
        assert_eq!(r.items[0].turns, [2]);
        assert_eq!(
            r.items[0].truth,
            "3か月以内の職種と人数: ドライバー 2名 / それ以外の募集: 事務 1名 (来年の春)"
        );
        assert_eq!(
            r.items[0].your_record,
            "3か月以内の職種と人数: ドライバー　２名 / それ以外の募集: 聞けていない"
        );
        assert_eq!(r.question_count, 2);
        assert_eq!(r.question_band, "1-4");
        assert!(!r.asked_price);
        assert_eq!(r.irritations.len(), 1);
        assert_eq!(r.irritations[0].turn, 5);
        assert_eq!(r.irritations[0].prev_rep_turn, Some(4));
        assert_eq!(r.irritations[0].customer_text, "それ、当日じゃだめですか");
    }

    #[test]
    fn 聞けていないと空欄と記録違いを分ける() {
        let tr = turns();
        let marks = vec![mark(2, &["hiring_3m"]), mark(4, &["decider", "attend"])];
        let mut record = rec(
            f("value", "ドライバー 3名"),
            f("empty", ""),
            f("not_heard", ""),
        );
        record.urgency = f("not_heard", "");
        record.consider = f("empty", "");
        record.budget = f("empty", "");
        record.decider = f("value", "self");
        record.attend = f("value", "yes");
        let req = PracticeFinishRequest {
            record,
            heat_guess: "cold".into(),
        };
        let r = build_result(&input(&tr, &marks, &[], 9), &req, None, None, None);
        let st: Vec<&str> = r.items.iter().map(|i| i.status.as_str()).collect();
        // ① 3名は違う / ② 聞けていないと記録 / ③ 空欄 / ④ 決める人が違う
        assert_eq!(st, ["wrong_record", "not_heard", "empty", "wrong_record"]);
        assert_eq!(r.not_heard_left, 2);
        assert_eq!(r.not_heard_items, ["② 課題と意欲", "③ 検討と予算"]);
        assert_eq!(r.question_band, "8-10");
    }

    #[test]
    fn 見立ての当たり外れと聞き出せた印を並べる() {
        let tr = turns();
        // 熱い型の印 5 つのうち、人数(複数名)だけ聞けた
        let marks = vec![mark(2, &["hiring_3m"])];
        let req = PracticeFinishRequest {
            record: rec(f("value", "ドライバー2名"), f("empty", ""), f("empty", "")),
            heat_guess: "cold".into(),
        };
        let r = build_result(&input(&tr, &marks, &[], 1), &req, None, None, None);
        assert_eq!(r.heat.truth, "hot");
        assert_eq!(r.heat.hot_marks_hidden, 5);
        assert_eq!(r.heat.hot_marks_heard, 1);
        assert_eq!(r.heat.implied_from_heard, "cold");
        assert!(!r.heat.guess_matches_truth);
        assert!(r
            .heat
            .message
            .contains("聞き出せた答えからは「冷めている」が妥当"));
        assert!(r.heat.message.contains("本当の型は「熱い」"));
        assert_eq!(r.persona.label, "熱い型");
        assert_eq!(r.persona.hidden.len(), 8);
    }

    #[test]
    fn 予算なし型は月額定額を聞いた後の答えで照らし合わせる() {
        let tr = turns();
        let p = find("budget_01").unwrap();
        let marks = vec![mark(4, &["consider", "budget"])];
        let mk = |fee: bool, at: Option<&str>, v: &str| {
            let mut record = rec(f("empty", ""), f("empty", ""), f("empty", ""));
            record.consider = f("value", "depends");
            record.budget = f("value", v);
            let req = PracticeFinishRequest {
                record,
                heat_guess: "mixed".into(),
            };
            let inp = ScoreInput {
                persona: p,
                transcript: &tr,
                marks: &marks,
                irritated_turns: &[],
                question_count: 2,
                fee_told: fee,
                budget_at_ask: at.map(String::from),
                script_seen: false,
            };
            build_result(&inp, &req, None, None, None)
        };
        assert_eq!(mk(true, Some("yes"), "yes").items[2].status, "got");
        assert_eq!(mk(false, Some("no"), "no").items[2].status, "got");
        assert_eq!(mk(false, Some("no"), "yes").items[2].status, "wrong_record");
        let r = mk(true, Some("yes"), "yes");
        assert_eq!(r.heat.truth, "mixed");
        assert!(r.heat.guess_matches_truth);
        assert!(r.items[2].truth.contains("最初は「無い」と言い"));
    }

    #[test]
    fn 質問の数の帯() {
        assert_eq!(question_band(4), "1-4");
        assert_eq!(question_band(5), "5-7");
        assert_eq!(question_band(7), "5-7");
        assert_eq!(question_band(10), "8-10");
        assert_eq!(question_band(11), "11+");
    }

    struct FakeC(Vec<Value>, std::sync::Mutex<usize>);
    impl Completer for FakeC {
        async fn complete(&self, _p: &str) -> Result<Value, JudgeError> {
            let mut i = self.1.lock().unwrap();
            let v = self.0[(*i).min(self.0.len() - 1)].clone();
            *i += 1;
            Ok(v)
        }
    }

    #[tokio::test]
    async fn 元の発言と同じ言い直しは出さない() {
        let tr = turns();
        let req = PracticeFinishRequest {
            record: rec(f("empty", ""), f("empty", ""), f("value", "前任が辞めた")),
            heat_guess: "hot".into(),
        };
        let v = serde_json::json!({
            "summary": "月額定額を先に伝えるとよいです",
            "good": [],
            "bad": [{"turn": 4, "comment": "金額を直接聞いています", "better": "予算はいくらですか"}],
            "issue_match": null
        });
        let c = FakeC(vec![v], std::sync::Mutex::new(0));
        let inp = input(&tr, &[], &[], 2);
        let (com, _) = commentary_via(&c, &inp, &req).await.unwrap();
        assert_eq!(com.bad.len(), 1);
        assert_eq!(com.bad[0].comment, "金額を直接聞いています");
        assert_eq!(com.bad[0].better, None);
    }

    #[tokio::test]
    async fn コメントは営業の発言の番号と日本語だけを残す() {
        let tr = turns();
        let marks = vec![mark(4, &["budget"])];
        let req = PracticeFinishRequest {
            record: rec(f("empty", ""), f("empty", ""), f("value", "前任が辞めた")),
            heat_guess: "hot".into(),
        };
        let bad_first =
            serde_json::json!({"summary":"这个问题","good":[],"bad":[],"issue_match":null});
        let ok = serde_json::json!({
            "summary": "金額を直接聞かず、月額定額を先に伝えるとよいです",
            "good": [{"turn": 2, "comment": "3か月以内と区切って聞けています"}, {"turn": 3, "comment": "先方の発言は対象外"}],
            "bad": [{"turn": 4, "comment": "金額を直接聞いています", "better": "弊社は月額定額のサービスです。その形でご検討いただけそうですか"},
                    {"turn": 99, "comment": "存在しない番号"}, {"turn": 4, "comment": "必ず直す"}],
            "issue_match": true
        });
        let c = FakeC(vec![bad_first, ok], std::sync::Mutex::new(0));
        let inp = input(&tr, &marks, &[], 2);
        let (com, im) = commentary_via(&c, &inp, &req).await.unwrap();
        assert_eq!(
            *c.1.lock().unwrap(),
            2,
            "日本語以外が混ざったので 1 回やり直す"
        );
        assert_eq!(im, Some(true));
        assert_eq!(com.good.len(), 1);
        assert_eq!(com.good[0].turn, 2);
        assert_eq!(
            com.good[0].text,
            "直近3か月以内で、採用が必要な職種はありますか？"
        );
        assert_eq!(com.bad.len(), 1);
        assert_eq!(com.bad[0].turn, 4);
        assert!(com.bad[0].better.as_deref().unwrap().contains("月額定額"));
        assert!(com.summary.unwrap().contains("金額を直接聞かず"));
    }

    #[test]
    fn 課題の趣旨の判定は規則のキーワードより優先する() {
        let tr = turns();
        let marks = vec![mark(2, &["issue", "urgency"])];
        let req = PracticeFinishRequest {
            record: rec(f("empty", ""), f("empty", ""), f("value", "配車が回らない")),
            heat_guess: "hot".into(),
        };
        let inp = input(&tr, &marks, &[], 1);
        let by_rule = build_result(&inp, &req, None, None, None);
        assert_eq!(by_rule.items[1].status, "got");
        let by_llm = build_result(&inp, &req, None, None, Some(false));
        assert_eq!(by_llm.items[1].status, "wrong_record");
    }
}
