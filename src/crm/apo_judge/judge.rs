//! アポの質の判定 (MiniMax)。プロンプトは `prompt.md` (差し替え用)、出力の読み取りと区分の決定はここ。
//!
//! 点数と区分は MiniMax の自己申告を使わず、条件の有無から決める (点数 = 該当した条件の数、区分は下の規則)。
//! モデルの `score` / `grade` が食い違っても無視する。

use serde_json::Value;

use crate::crm::talk_review::judge::{Completer, JudgeError, LANG_RETRIES};
use crate::crm::talk_review::lang::{find_foreign, FALLBACK_TEXT};
use crate::crm::talk_review::types::TalkTurn;

pub const PROMPT: &str = include_str!("prompt.md");

/// 区分が「同席推奨」になる点数 (仮)
pub const ATTEND_MIN_SCORE: u32 = 4;
/// 文字起こしを前から切る文字数
pub const MAX_TRANSCRIPT_CHARS: usize = 20_000;
const MAX_QUOTE_CHARS: usize = 80;
const MAX_REASON_CHARS: usize = 300;

const STRICT_SUFFIX: &str = "\n\n重要: reason は必ず日本語だけで書く。中国語・韓国語・英語の単語を混ぜない(Indeed、Zoom などの固有名詞は可)。";

/// (キー, 通知に出す表示名)
pub const CONDITIONS: [(&str, &str); 5] = [
    ("budget", "予算の金額の目安"),
    ("timing", "入社時期が3か月以内"),
    ("decision_maker", "経営層・部長所長級"),
    ("vacancy_reason", "募集背景が欠員"),
    ("competing", "検討中の他サービスあり"),
];

/// (キー, 表示名)
pub const LAYERS: [(&str, &str); 5] = [
    ("not_recruiting", "募集していない"),
    ("no_budget", "費用をかけられない"),
    ("hq_managed", "本社一括管理"),
    ("success_fee_only", "成果報酬型のみ"),
    ("no_issue", "課題なし"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Warning,
    Normal,
    Attend,
}

impl Grade {
    /// HubSpot の選択肢の内部値 (`scripts/ensure_apo_ai_properties.py` と同じ)
    pub fn value(self) -> &'static str {
        match self {
            Grade::Warning => "warning",
            Grade::Normal => "normal",
            Grade::Attend => "attend",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Grade::Warning => "警告",
            Grade::Normal => "通常",
            Grade::Attend => "同席推奨",
        }
    }
}

/// アポ化しない層があれば警告、点数が `ATTEND_MIN_SCORE` 以上なら同席推奨、それ以外は通常
pub fn grade_of(score: u32, excluded: &[&'static str]) -> Grade {
    if !excluded.is_empty() {
        Grade::Warning
    } else if score >= ATTEND_MIN_SCORE {
        Grade::Attend
    } else {
        Grade::Normal
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConditionResult {
    pub key: &'static str,
    pub label: &'static str,
    pub present: bool,
    pub quote: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApoJudgement {
    pub conditions: Vec<ConditionResult>,
    /// 該当したアポ化しない層のキー (`LAYERS` の順)
    pub excluded_keys: Vec<&'static str>,
    pub score: u32,
    pub grade: Grade,
    pub reason: String,
}

impl ApoJudgement {
    /// HubSpot の `apo_ai_reason` (複数行テキスト) に書く文面
    pub fn reason_text(&self) -> String {
        let mut s = format!("区分: {} / 点数: {}/5\n", self.grade.label(), self.score);
        for c in &self.conditions {
            s.push_str(if c.present { "○ " } else { "× " });
            s.push_str(c.label);
            if let Some(q) = c.quote.as_deref().filter(|_| c.present) {
                s.push_str(&format!("「{q}」"));
            }
            s.push('\n');
        }
        if !self.excluded_keys.is_empty() {
            let names: Vec<&str> = LAYERS
                .iter()
                .filter(|(k, _)| self.excluded_keys.contains(k))
                .map(|(_, n)| *n)
                .collect();
            s.push_str(&format!("アポ化しない層: {}\n", names.join("、")));
        }
        s.push_str(&format!("理由: {}", self.reason));
        s
    }
}

fn clip(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

/// モデルの JSON を読む。条件が欠けている・present が真偽値でない・JSON の形が違うときは `None`
pub fn parse_judgement(v: &Value) -> Option<ApoJudgement> {
    let conds = v.get("conditions")?.as_object()?;
    let mut conditions = Vec::new();
    for (key, label) in CONDITIONS {
        let c = conds.get(key)?;
        let present = c.get("present")?.as_bool()?;
        let quote = c
            .get("quote")
            .and_then(Value::as_str)
            .map(|q| clip(q, MAX_QUOTE_CHARS))
            .filter(|q| !q.is_empty());
        conditions.push(ConditionResult {
            key,
            label,
            present,
            quote,
        });
    }
    let layers = v.get("excluded_layers")?.as_array()?;
    // 知らない名前は無視する (絞り込みすぎない)
    let excluded_keys: Vec<&'static str> = LAYERS
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| layers.iter().any(|l| l.as_str() == Some(k)))
        .collect();
    let score = conditions.iter().filter(|c| c.present).count() as u32;
    let grade = grade_of(score, &excluded_keys);
    let reason = clip(
        v.get("reason").and_then(Value::as_str).unwrap_or(""),
        MAX_REASON_CHARS,
    );
    Some(ApoJudgement {
        conditions,
        excluded_keys,
        score,
        grade,
        reason,
    })
}

pub fn build_prompt(turns: &[TalkTurn]) -> String {
    let mut tr = String::new();
    for t in turns {
        let line = format!(
            "{}. {}: {}\n",
            t.n,
            if t.speaker == "rep" { "営" } else { "先" },
            t.text
        );
        if tr.chars().count() + line.chars().count() > MAX_TRANSCRIPT_CHARS {
            break;
        }
        tr.push_str(&line);
    }
    PROMPT.replace("{{TRANSCRIPT}}", tr.trim_end())
}

/// 判定して、reason に日本語以外が混ざっていたら念押しして最大 `LANG_RETRIES` 回やり直す。
/// それでも混ざるときは reason を定型文にする (点数・区分は残す)。形の違う応答は 1 回だけやり直す
pub async fn judge_apo<C: Completer>(
    c: &C,
    turns: &[TalkTurn],
) -> Result<ApoJudgement, JudgeError> {
    let base = build_prompt(turns);
    let source: String = turns
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut last: Option<ApoJudgement> = None;
    let mut bad_shape = 0;
    for attempt in 0..=LANG_RETRIES {
        let prompt = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}{STRICT_SUFFIX}")
        };
        let v = match c.complete(&prompt).await {
            Ok(v) => v,
            Err(e) => {
                return match last {
                    Some(j) => Ok(sanitize(j, &source)),
                    None => Err(e),
                }
            }
        };
        let Some(j) = parse_judgement(&v) else {
            bad_shape += 1;
            if bad_shape >= 2 {
                return match last {
                    Some(j) => Ok(sanitize(j, &source)),
                    None => Err(JudgeError::Transient("invalid json shape".into())),
                };
            }
            continue;
        };
        if find_foreign(&j.reason, &source).is_none() {
            return Ok(j);
        }
        last = Some(j);
    }
    match last {
        Some(j) => Ok(sanitize(j, &source)),
        None => Err(JudgeError::Transient("invalid json shape".into())),
    }
}

fn sanitize(mut j: ApoJudgement, source: &str) -> ApoJudgement {
    if find_foreign(&j.reason, source).is_some() || j.reason.is_empty() {
        j.reason = FALLBACK_TEXT.to_string();
    }
    j
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    fn turns() -> Vec<TalkTurn> {
        vec![
            TalkTurn {
                n: 1,
                speaker: "rep".into(),
                text: "採用の背景を教えてください".into(),
            },
            TalkTurn {
                n: 2,
                speaker: "customer".into(),
                text: "退職者が出たので補充です。予算は月30万円ほど".into(),
            },
        ]
    }

    fn good() -> Value {
        json!({
            "conditions": {
                "budget": {"present": true, "quote": "予算は月30万円ほど"},
                "timing": {"present": false, "quote": null},
                "decision_maker": {"present": true, "quote": "部長の山田です"},
                "vacancy_reason": {"present": true, "quote": "退職者が出た"},
                "competing": {"present": true, "quote": "他社にも相談中"}
            },
            "excluded_layers": [],
            "score": 1,
            "grade": "警告",
            "reason": "予算と背景が具体的に出ています。"
        })
    }

    #[test]
    fn score_and_grade_come_from_conditions_not_from_the_model() {
        let j = parse_judgement(&good()).unwrap();
        // モデルの score=1 / grade=警告 は無視し、○ の数 4 から 同席推奨
        assert_eq!(j.score, 4);
        assert_eq!(j.grade, Grade::Attend);
        assert_eq!(j.grade.value(), "attend");
        assert_eq!(j.conditions[1].quote, None);
    }

    #[test]
    fn excluded_layer_makes_a_warning_and_unknown_layers_are_ignored() {
        let mut v = good();
        v["excluded_layers"] = json!(["hq_managed", "something_new"]);
        let j = parse_judgement(&v).unwrap();
        assert_eq!(j.excluded_keys, vec!["hq_managed"]);
        assert_eq!(j.grade, Grade::Warning);
        assert!(j.reason_text().contains("アポ化しない層: 本社一括管理"));
    }

    #[test]
    fn grade_thresholds() {
        assert_eq!(grade_of(3, &[]), Grade::Normal);
        assert_eq!(grade_of(4, &[]), Grade::Attend);
        assert_eq!(grade_of(5, &["no_issue"]), Grade::Warning);
        assert_eq!(grade_of(0, &[]), Grade::Normal);
    }

    #[test]
    fn invalid_shapes_are_rejected() {
        let mut v = good();
        v["conditions"]["budget"]["present"] = json!("yes");
        assert!(parse_judgement(&v).is_none());
        let mut v = good();
        v["conditions"].as_object_mut().unwrap().remove("timing");
        assert!(parse_judgement(&v).is_none());
        let mut v = good();
        v["excluded_layers"] = json!("none");
        assert!(parse_judgement(&v).is_none());
        assert!(parse_judgement(&json!([])).is_none());
    }

    #[test]
    fn reason_text_has_marks_quotes_and_reason() {
        let t = parse_judgement(&good()).unwrap().reason_text();
        assert_eq!(
            t,
            "区分: 同席推奨 / 点数: 4/5\n○ 予算の金額の目安「予算は月30万円ほど」\n× 入社時期が3か月以内\n○ 経営層・部長所長級「部長の山田です」\n○ 募集背景が欠員「退職者が出た」\n○ 検討中の他サービスあり「他社にも相談中」\n理由: 予算と背景が具体的に出ています。"
        );
    }

    #[test]
    fn prompt_embeds_the_transcript() {
        let p = build_prompt(&turns());
        assert!(p.contains("2. 先: 退職者が出たので補充です。予算は月30万円ほど"));
        assert!(!p.contains("{{TRANSCRIPT}}"));
    }

    struct Fake(Mutex<Vec<Value>>, Mutex<Vec<String>>);
    impl Completer for Fake {
        async fn complete(&self, prompt: &str) -> Result<Value, JudgeError> {
            self.1.lock().unwrap().push(prompt.to_string());
            let mut q = self.0.lock().unwrap();
            if q.is_empty() {
                return Err(JudgeError::Transient("empty".into()));
            }
            Ok(q.remove(0))
        }
    }

    #[tokio::test]
    async fn foreign_reason_is_retried_then_replaced() {
        let mut bad = good();
        bad["reason"] = json!("这个电话很好");
        let f = Fake(
            Mutex::new(vec![bad.clone(), bad.clone(), bad]),
            Mutex::new(vec![]),
        );
        let j = judge_apo(&f, &turns()).await.unwrap();
        assert_eq!(j.reason, FALLBACK_TEXT);
        assert_eq!(j.score, 4);
        let calls = f.1.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(!calls[0].contains("重要: reason"));
        assert!(calls[1].contains("重要: reason"));
    }

    #[tokio::test]
    async fn broken_json_twice_is_an_error_and_once_is_retried() {
        let f = Fake(Mutex::new(vec![json!({}), json!({})]), Mutex::new(vec![]));
        assert!(judge_apo(&f, &turns()).await.is_err());
        let f = Fake(Mutex::new(vec![json!({}), good()]), Mutex::new(vec![]));
        assert_eq!(judge_apo(&f, &turns()).await.unwrap().score, 4);
    }
}
