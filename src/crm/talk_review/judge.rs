//! 文字起こしを `checks.json` の項目で判定する (MiniMax、OpenAI 互換)。
//! プロンプトは試作 (`scripts/call_advice/call_advice.py` の `build_prompt`) と同じ。
//! 呼び出し部分は `Judge` に分けてあり、テストでは偽物に差し替える。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::checks::{Check, Checks};
use super::types::TalkTurn;

/// 結果に残す短い引用の最大文字数
const QUOTE_MAX_CHARS: usize = 200;

/// 1 項目の判定 (表に JSON で保存する形。status は done / not_done / na)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredItem {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub evidence: Vec<u32>,
    #[serde(default)]
    pub quote: Option<String>,
    #[serde(default)]
    pub advice: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeOutput {
    pub reached_person: Option<bool>,
    pub reception: Option<bool>,
    pub first_date_turn: Option<u32>,
    /// 日程が決まった / 決まらなかった / 担当者不在
    pub outcome: Option<String>,
    /// checks の順
    pub items: Vec<StoredItem>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgeError {
    /// 通信・混雑・応答の読み取り失敗 (後で再試行)
    Transient(String),
    /// 鍵が無いなど、設定の問題
    NotConfigured,
}

pub trait Judge: Send + Sync {
    fn judge(
        &self,
        checks: &Checks,
        turns: &[TalkTurn],
    ) -> impl std::future::Future<Output = Result<JudgeOutput, JudgeError>> + Send;
}

// ---------------------------------------------------------------------------
// プロンプト (試作の build_prompt と同じ)
// ---------------------------------------------------------------------------

pub fn build_prompt(checks: &Checks, turns: &[TalkTurn]) -> String {
    let items: Vec<String> = checks
        .checks
        .iter()
        .map(|c| {
            format!(
                "- id: {}\n  項目: {}\n  判定: {}\n  該当なし: {}",
                c.id, c.title, c.rule, c.na_when
            )
        })
        .collect();
    let tr: Vec<String> = turns
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
    format!(
        r#"あなたは新規営業のアポ取り電話を評価する担当です。下の文字起こし(営=営業、先=先方。番号は発言の順番)を、項目ごとに判定してください。

決まり:
- アポが決まった後の会話(日時の復唱、Zoom・メールアドレスの確認など)は判定に使わない。決まったかどうかは outcome に書くだけにする。
- 判定は「できた」「できていない」「該当なし」のどれか。文字起こしが崩れていて判断できないときは「該当なし」にして note に理由を書く。
- evidence には判定の根拠になった発言の番号を入れる。quote にはその発言をそのまま短く引用する。
- advice は「できていない」ときだけ、その電話の流れに合わせて、営業が次に言うとよい一言を具体的に書く。資料の言い方を参考にする。
- 平易な日本語で書く。

項目:
{items}

文字起こし:
{tr}

次の JSON だけを返してください:
{{"reached_person": true/false (担当者と話せたか),
 "reception": true/false (受付を通したか),
 "first_date_turn": 営業が最初に具体的な日付を出した発言の番号 または null,
 "outcome": "日程が決まった" / "決まらなかった" / "担当者不在" のどれか,
 "checks": [{{"id": "...", "status": "...", "evidence": [番号], "quote": "...", "advice": "...", "note": "..."}}],
 "summary": "この電話で一番直すとよい点を1文で"}}"#,
        items = items.join("\n"),
        tr = tr.join("\n"),
    )
}

// ---------------------------------------------------------------------------
// 応答の読み取り
// ---------------------------------------------------------------------------

/// `<think>…</think>` とコードブロックを剥がして JSON を取り出す (minimax_gateway.py の `_extract_json` と同じ)
pub fn extract_json(text: &str) -> Option<Value> {
    let mut s = text.to_string();
    while let (Some(a), Some(b)) = (s.find("<think>"), s.find("</think>")) {
        if b < a {
            break;
        }
        s.replace_range(a..b + "</think>".len(), "");
    }
    // 閉じていない <think> は先頭から捨てる
    if let Some(a) = s.find("<think>") {
        if !s.contains("</think>") {
            s.truncate(a);
        }
    }
    let mut s = s.trim().to_string();
    if let Some(a) = s.find("```") {
        let rest = &s[a + 3..];
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        if let Some(b) = rest.find("```") {
            s = rest[..b].trim().to_string();
        }
    }
    let (i, j) = (s.find('{')?, s.rfind('}')?);
    if j <= i {
        return None;
    }
    if let Ok(v) = serde_json::from_str::<Value>(&s[i..=j]) {
        return Some(v);
    }
    // 末尾が余分なとき、対応が取れる最大の範囲を試す
    s[i..=j]
        .char_indices()
        .filter(|(_, c)| *c == '}')
        .rev()
        .find_map(|(k, _)| serde_json::from_str::<Value>(&s[i..=i + k]).ok())
}

/// 「できた / できていない / 該当なし」→ done / not_done / na。分からない値は na
pub fn status_from_ja(s: &str) -> &'static str {
    match s.trim() {
        "できた" => "done",
        "できていない" => "not_done",
        _ => "na",
    }
}

fn trunc(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn text_of(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(super) fn bool_of(v: &Value, key: &str) -> Option<bool> {
    match v.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim() {
            "true" | "はい" => Some(true),
            "false" | "いいえ" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn number_of(v: &Value) -> Option<u32> {
    match v {
        Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn convert_item(c: &Check, raw: Option<&Value>) -> StoredItem {
    let Some(r) = raw else {
        return StoredItem {
            id: c.id.clone(),
            status: "na".into(),
            evidence: vec![],
            quote: None,
            advice: None,
            note: Some("判定の応答にこの項目がありませんでした".into()),
        };
    };
    let status = status_from_ja(r.get("status").and_then(Value::as_str).unwrap_or(""));
    let evidence = match r.get("evidence") {
        Some(Value::Array(a)) => a.iter().filter_map(number_of).collect(),
        Some(v) => number_of(v).into_iter().collect(),
        None => vec![],
    };
    StoredItem {
        id: c.id.clone(),
        status: status.to_string(),
        evidence,
        quote: text_of(r, "quote").map(|q| trunc(&q, QUOTE_MAX_CHARS)),
        advice: text_of(r, "advice").filter(|_| status == "not_done"),
        note: text_of(r, "note"),
    }
}

/// モデルの JSON → 保存する形。checks の順に並べ、欠けた項目は na にする
pub fn convert_response(checks: &Checks, v: &Value) -> JudgeOutput {
    let arr = v.get("checks").and_then(Value::as_array);
    let find = |id: &str| {
        arr.and_then(|a| {
            a.iter()
                .find(|x| x.get("id").and_then(Value::as_str) == Some(id))
        })
    };
    let outcome = text_of(v, "outcome").filter(|o| {
        matches!(
            o.as_str(),
            "日程が決まった" | "決まらなかった" | "担当者不在"
        )
    });
    JudgeOutput {
        reached_person: bool_of(v, "reached_person"),
        reception: bool_of(v, "reception"),
        first_date_turn: v.get("first_date_turn").and_then(number_of),
        outcome,
        items: checks
            .checks
            .iter()
            .map(|c| convert_item(c, find(&c.id)))
            .collect(),
        summary: text_of(v, "summary"),
    }
}

// ---------------------------------------------------------------------------
// MiniMax
// ---------------------------------------------------------------------------

pub struct MiniMaxJudge {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl MiniMaxJudge {
    /// `MINIMAX_API_KEY` が無ければ None
    pub fn new(base_url: &str, model: &str) -> Option<Self> {
        let api_key = std::env::var("MINIMAX_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        Some(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .ok()?,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model: model.to_string(),
        })
    }

    pub fn request_body(model: &str, prompt: &str) -> Value {
        json!({
            "model": model,
            "temperature": 0,
            "max_tokens": 24000,
            "messages": [{"role": "user", "content":
                format!("{prompt}\n\n説明やコードブロックは付けず、JSON だけを出力してください。")}],
        })
    }
}

impl MiniMaxJudge {
    /// プロンプトを送り、応答から JSON を取り出す。混雑・通信の失敗は 3 回まで間を空けて試し直す。
    /// 混雑 (429) で終わったときの理由は `http 429`
    pub async fn complete_json(&self, prompt: &str) -> Result<Value, JudgeError> {
        let body = Self::request_body(&self.model, prompt);
        let mut last = String::new();
        for attempt in 0..3u64 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(3 * attempt)).await;
            }
            let r = match self
                .http
                .post(format!("{}/chat/completions", self.base_url))
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last = format!("send: {}", e.without_url());
                    continue;
                }
            };
            let st = r.status().as_u16();
            if matches!(st, 429 | 500 | 502 | 503 | 504) {
                last = format!("http {st}");
                continue;
            }
            if !r.status().is_success() {
                return Err(JudgeError::Transient(format!("http {st}")));
            }
            let Ok(v) = r.json::<Value>().await else {
                last = "body".into();
                continue;
            };
            let content = v
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .unwrap_or("");
            match extract_json(content) {
                Some(j) => return Ok(j),
                None => last = "json".into(),
            }
        }
        Err(JudgeError::Transient(last))
    }
}

impl Judge for MiniMaxJudge {
    async fn judge(&self, checks: &Checks, turns: &[TalkTurn]) -> Result<JudgeOutput, JudgeError> {
        let j = self.complete_json(&build_prompt(checks, turns)).await?;
        Ok(convert_response(checks, &j))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::checks::loaded;

    #[test]
    fn extract_json_handles_think_and_fences() {
        let t = "<think>考え中 {\"x\":1}</think>\n```json\n{\"a\": 1}\n```";
        assert_eq!(extract_json(t).unwrap()["a"], json!(1));
        assert_eq!(
            extract_json("前置き {\"a\": 2} 後ろ").unwrap()["a"],
            json!(2)
        );
        assert!(extract_json("JSON なし").is_none());
        assert!(extract_json("<think>途中で切れた").is_none());
    }

    #[test]
    fn status_mapping() {
        assert_eq!(status_from_ja("できた"), "done");
        assert_eq!(status_from_ja("できていない"), "not_done");
        assert_eq!(status_from_ja("該当なし"), "na");
        assert_eq!(status_from_ja("???"), "na");
    }

    #[test]
    fn converts_model_response_in_checks_order() {
        let c = &loaded().checks;
        let v = json!({
            "reached_person": true, "reception": "false", "first_date_turn": 7,
            "outcome": "決まらなかった",
            "checks": [
                {"id": "early_date", "status": "できていない", "evidence": [7, "8"],
                 "quote": "来週はいかがですか", "advice": "先に内容を一言伝える", "note": ""},
                {"id": "reception_name", "status": "該当なし", "evidence": [], "quote": "", "advice": "不要", "note": "受付なし"},
                {"id": "busy_reoffer", "status": "できた", "evidence": [3], "advice": "出ても捨てる"}
            ],
            "summary": "日付が早い"
        });
        let o = convert_response(c, &v);
        assert_eq!(o.items.len(), 9);
        assert_eq!(o.items[0].id, "reception_name");
        assert_eq!(o.items[0].status, "na");
        assert_eq!(o.items[0].advice, None);
        assert_eq!(o.items[0].note.as_deref(), Some("受付なし"));
        let early = &o.items[2];
        assert_eq!(early.id, "early_date");
        assert_eq!(early.status, "not_done");
        assert_eq!(early.evidence, vec![7, 8]);
        assert_eq!(early.advice.as_deref(), Some("先に内容を一言伝える"));
        let busy = o.items.iter().find(|i| i.id == "busy_reoffer").unwrap();
        assert_eq!(busy.status, "done");
        assert_eq!(busy.advice, None, "できたときの助言は残さない");
        // 応答に無い項目は na
        let k = o.items.iter().find(|i| i.id == "kizonkan").unwrap();
        assert_eq!(k.status, "na");
        assert_eq!(o.reached_person, Some(true));
        assert_eq!(o.reception, Some(false));
        assert_eq!(o.first_date_turn, Some(7));
        assert_eq!(o.outcome.as_deref(), Some("決まらなかった"));
        assert_eq!(o.summary.as_deref(), Some("日付が早い"));
    }

    #[test]
    fn unknown_outcome_is_dropped_and_quote_truncated() {
        let c = &loaded().checks;
        let long = "あ".repeat(500);
        let v = json!({"outcome": "不明", "checks": [{"id": "early_date", "status": "できた", "quote": long}]});
        let o = convert_response(c, &v);
        assert_eq!(o.outcome, None);
        assert_eq!(o.items[2].quote.as_ref().unwrap().chars().count(), 200);
    }

    #[test]
    fn prompt_contains_items_and_numbered_turns() {
        let c = &loaded().checks;
        let turns = vec![
            TalkTurn {
                n: 1,
                speaker: "rep".into(),
                text: "もしもし".into(),
            },
            TalkTurn {
                n: 2,
                speaker: "customer".into(),
                text: "はい".into(),
            },
        ];
        let p = build_prompt(c, &turns);
        assert!(p.contains("- id: reception_name\n  項目: 受付で担当者を名前で呼ぶ"));
        assert!(p.contains("1. 営: もしもし\n2. 先: はい"));
        assert!(p.contains("\"outcome\": \"日程が決まった\""));
        let b = MiniMaxJudge::request_body("MiniMax-M3", "P");
        assert_eq!(b["temperature"], json!(0));
        assert_eq!(b["model"], json!("MiniMax-M3"));
    }
}
