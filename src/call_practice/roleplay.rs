//! お客さん役。Claude (Messages API の生 HTTP、ストリーミング) に型を渡して演じさせる。
//! 返事の文を先に流し、最後の行の `###{...}` に営業が聞いた項目などの印を付けさせる。

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use super::persona::{Persona, SLOTS};

pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MARKER: &str = "###";
/// 返事 1 回の待ちの上限
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMsg {
    pub role: &'static str,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// 通信・混雑・応答の失敗
    Failed(String),
    /// モデルが答えるのを断った (stop_reason = refusal)
    Refusal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomerReply {
    pub text: String,
    pub asked_slots: Vec<String>,
    pub is_question: bool,
    pub irritated: bool,
    pub end: bool,
}

/// 流れてくる文字から、`###` 以降 (印の行) を除いた部分だけを取り出す。
/// `###` の途中まで来ている末尾は、続きが分かるまで出さない。
#[derive(Default)]
pub struct StreamFilter {
    full: String,
    emitted: usize,
    done: bool,
}

impl StreamFilter {
    pub fn push(&mut self, s: &str) -> Option<String> {
        self.full.push_str(s);
        if self.done {
            return None;
        }
        let visible_end = match self.full.find(MARKER) {
            Some(i) => {
                self.done = true;
                i
            }
            None => {
                // 末尾が "#" や "##" で終わるときは、次の文字を待つ
                let mut keep = 0;
                for k in (1..MARKER.len()).rev() {
                    if self.full.ends_with(&MARKER[..k]) {
                        keep = k;
                        break;
                    }
                }
                self.full.len() - keep
            }
        };
        if visible_end > self.emitted {
            let out = self.full[self.emitted..visible_end].to_string();
            self.emitted = visible_end;
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        } else {
            None
        }
    }

    /// 終わったとき、止めていた末尾を出す
    pub fn finish(&mut self) -> Option<String> {
        if self.done || self.emitted >= self.full.len() {
            return None;
        }
        let out = self.full[self.emitted..].to_string();
        self.emitted = self.full.len();
        Some(out)
    }

    pub fn full(&self) -> &str {
        &self.full
    }
}

/// 全文から返事と印を取り出す。印が読めなければ、返事の文だけを使い、聞いた項目は無し
pub fn parse_reply(full: &str) -> CustomerReply {
    let (body, tail) = match full.find(MARKER) {
        Some(i) => (&full[..i], Some(&full[i + MARKER.len()..])),
        None => (full, None),
    };
    let meta: Option<Value> = tail.and_then(|t| {
        let t = t.trim();
        let (a, b) = (t.find('{')?, t.rfind('}')?);
        serde_json::from_str(&t[a..=b]).ok()
    });
    let text = body.trim().to_string();
    let asked_slots: Vec<String> = meta
        .as_ref()
        .and_then(|m| m.get("asked_slots"))
        .and_then(Value::as_array)
        .map(|a| {
            let mut v: Vec<String> = vec![];
            for s in a.iter().filter_map(Value::as_str) {
                if SLOTS.contains(&s) && !v.iter().any(|x| x == s) {
                    v.push(s.to_string());
                }
            }
            v
        })
        .unwrap_or_default();
    let is_question = meta
        .as_ref()
        .and_then(|m| m.get("is_question"))
        .and_then(Value::as_bool)
        .unwrap_or(!asked_slots.is_empty());
    let irritated = meta
        .as_ref()
        .and_then(|m| m.get("mood"))
        .and_then(Value::as_str)
        == Some("irritated");
    let end = meta
        .as_ref()
        .and_then(|m| m.get("end"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    CustomerReply {
        text,
        asked_slots,
        is_question,
        irritated,
        end,
    }
}

// ---------------------------------------------------------------------------
// プロンプト
// ---------------------------------------------------------------------------

fn slot_label(slot: &str) -> &'static str {
    match slot {
        "hiring_3m" => "hiring_3m (3か月以内に採りたい職種と人数)",
        "hiring_later" => "hiring_later (それ以外の、先の募集)",
        "issue" => "issue (採用で困っていること)",
        "urgency" => "urgency (どのくらい早く解決したいか)",
        "consider" => "consider (月額定額のサービスを検討できそうか)",
        "budget" => "budget (予算の有無)",
        "decider" => "decider (最終的に決める人)",
        _ => "attend (オンライン商談への同席)",
    }
}

/// 練習の間は変わらない部分 (プロンプトキャッシュの対象)
pub fn build_system(p: &Persona) -> String {
    let mut s = String::new();
    s.push_str("あなたは電話口のお客さん役です。採用支援サービスの営業から電話を受け、オンライン商談の日時に了承したところです。営業の練習相手として、実際の先方のように短く答えます。助言・解説・採点はしません。\n\n");
    s.push_str(&format!(
        "## 人物 (架空)\n{}の{}。規模は{}。口調は「{}」。話の長さ(0〜3)は{}、答えるまでの渋さ(0〜3)は{}。\n\n",
        p.setting.industry,
        p.setting.speaker_title,
        p.setting.size,
        p.personality.tone,
        p.personality.talkative,
        p.personality.guard
    ));
    s.push_str("## 隠れた答え (聞かれた項目だけ、この内容のとおりに答える。内容を変えない)\n");
    for slot in SLOTS {
        let say = match p.text_answer(slot) {
            Some(t) => t.say.as_str(),
            None => p.choice(slot).map(|c| c.say.as_str()).unwrap_or(""),
        };
        s.push_str(&format!("- {}: {}\n", slot_label(slot), say));
    }
    s.push_str("\n## 決まり\n");
    s.push_str("1. 聞かれた項目だけに答える。1回の返事で答える項目は1つ (話の長さが2以上なら2つまで)。聞かれていない項目は自分から言わない。\n");
    s.push_str("2. あいまいに聞かれたら、あいまいに答える (例: 「人は足りていますか」には「まあ、ぼちぼちですね」)。具体的に聞かれたときに答えの中身を出す。\n");
    s.push_str("3. 隠れた答えに無いことを聞かれたら、人物に合わせて「特にないです」「分からないです」と答える。\n");
    s.push_str(
        "4. 返事は80文字まで。話し言葉の日本語だけで書く。英単語・中国語・韓国語を混ぜない。\n",
    );
    s.push_str("5. 各ターンの最後に付く「進行メモ」に従う。嫌がる段階が「嫌がり始める」なら、短く「それ、当日じゃだめですか」のように嫌がる。「会話を閉じる」なら、挨拶して電話を切る方向の返事にして end を true にする。\n");
    s.push_str("6. 営業が締めの挨拶 (「ありがとうございました」「失礼いたします」など) をしたら、挨拶を返して end を true にする。\n");
    for n in &p.style_notes {
        s.push_str(&format!("7. この人物の決まり: {n}\n"));
    }
    if !p.phrases.is_empty() {
        s.push_str("\n## この人物の言い回しの例 (そのまま使ってもよい)\n");
        for ph in &p.phrases {
            s.push_str(&format!("- {ph}\n"));
        }
    }
    s.push_str(
        "\n## 出力の形\n返事の文だけを先に書く。そのあと改行して、最後の行に次の形で印を付ける。\n",
    );
    s.push_str(
        "###{\"asked_slots\":[...],\"is_question\":true,\"mood\":\"normal\",\"end\":false}\n",
    );
    s.push_str("- asked_slots: 営業が今回の発言で聞いた項目 (hiring_3m, hiring_later, issue, urgency, consider, budget, decider, attend のうち該当するもの。聞いていなければ空)\n");
    s.push_str("- is_question: 営業の発言が質問か (あいさつ・相づち・確認だけなら false)\n");
    s.push_str("- mood: 嫌がった返事なら \"irritated\"、それ以外は \"normal\"\n");
    s.push_str("- end: 会話を閉じるなら true\n");
    s.push_str("印の行以外に説明を書かない。\n");
    s
}

/// ターンごとに変わる進行メモ。営業の発言の後ろに足す
pub fn progress_memo(question_count: u32, p: &Persona, fee_told: bool) -> String {
    let patience = p.personality.patience;
    let stage = if question_count > patience + 3 {
        "会話を閉じる"
    } else if question_count > patience {
        "嫌がり始める"
    } else {
        "なし"
    };
    format!(
        "\n\n(進行メモ: 営業の質問はここまで{}問。月額定額は{}。嫌がる段階: {}。)",
        question_count,
        if fee_told {
            "伝えられた"
        } else {
            "まだ伝えられていない"
        },
        stage
    )
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
    url: String,
}

impl AnthropicClient {
    /// `ANTHROPIC_API_KEY` が無ければ None。モデルは `CALL_PRACTICE_MODEL`
    pub fn from_env() -> Option<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        let model = std::env::var("CALL_PRACTICE_MODEL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        Self::new(key, model, API_URL.to_string())
    }

    pub fn new(api_key: String, model: String, url: String) -> Option<Self> {
        Some(Self {
            http: reqwest::Client::builder()
                .timeout(REPLY_TIMEOUT)
                .build()
                .ok()?,
            api_key,
            model,
            url,
        })
    }

    pub fn request_body(model: &str, system: &str, msgs: &[ChatMsg]) -> Value {
        // temperature などのサンプリング引数は送らない (400 になる)
        json!({
            "model": model,
            "max_tokens": 400,
            "stream": true,
            "thinking": {"type": "between_tools"},
            "output_config": {"effort": "low"},
            "system": [{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}],
            "messages": msgs.iter().map(|m| json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
        })
    }

    async fn stream(
        &self,
        system: &str,
        msgs: &[ChatMsg],
        tx: &UnboundedSender<String>,
    ) -> Result<String, LlmError> {
        let body = Self::request_body(&self.model, system, msgs);
        let mut resp = self
            .http
            .post(&self.url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Failed(format!("send: {}", e.without_url())))?;
        if !resp.status().is_success() {
            return Err(LlmError::Failed(format!("http {}", resp.status().as_u16())));
        }
        let mut filter = StreamFilter::default();
        let mut buf = String::new();
        let mut stop_reason: Option<String> = None;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| LlmError::Failed(format!("read: {}", e.without_url())))?
        {
            buf.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));
            while let Some(i) = buf.find("\n\n") {
                let block: String = buf.drain(..i + 2).collect();
                for line in block.lines() {
                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let Ok(v) = serde_json::from_str::<Value>(data.trim()) else {
                        continue;
                    };
                    match v.get("type").and_then(Value::as_str) {
                        Some("content_block_delta") => {
                            if v.pointer("/delta/type").and_then(Value::as_str)
                                == Some("text_delta")
                            {
                                if let Some(t) = v.pointer("/delta/text").and_then(Value::as_str) {
                                    if let Some(out) = filter.push(t) {
                                        let _ = tx.send(out);
                                    }
                                }
                            }
                        }
                        Some("message_delta") => {
                            if let Some(r) = v.pointer("/delta/stop_reason").and_then(Value::as_str)
                            {
                                stop_reason = Some(r.to_string());
                            }
                        }
                        Some("error") => {
                            let kind = v
                                .pointer("/error/type")
                                .and_then(Value::as_str)
                                .unwrap_or("error");
                            return Err(LlmError::Failed(kind.to_string()));
                        }
                        _ => {}
                    }
                }
            }
        }
        if stop_reason.as_deref() == Some("refusal") {
            return Err(LlmError::Refusal);
        }
        if let Some(out) = filter.finish() {
            let _ = tx.send(out);
        }
        if filter.full().trim().is_empty() {
            return Err(LlmError::Failed("empty".into()));
        }
        Ok(filter.full().to_string())
    }
}

/// 決まった返事を順に返す偽物 (テスト用)
pub struct FakeCustomer {
    replies: Mutex<VecDeque<Result<String, LlmError>>>,
    /// 受け取ったシステムプロンプトと最後の会話 (テストが確かめる)
    pub seen: Mutex<Vec<(String, Vec<ChatMsg>)>>,
}

impl FakeCustomer {
    pub fn new(replies: Vec<Result<String, LlmError>>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            seen: Mutex::new(vec![]),
        }
    }
}

pub enum Customer {
    Anthropic(AnthropicClient),
    Fake(FakeCustomer),
}

impl Customer {
    /// 返事を流しながら `tx` に送り、全文 (印つき) を返す
    pub async fn stream_reply(
        &self,
        system: &str,
        msgs: &[ChatMsg],
        tx: &UnboundedSender<String>,
    ) -> Result<String, LlmError> {
        match self {
            Customer::Anthropic(c) => c.stream(system, msgs, tx).await,
            Customer::Fake(f) => {
                f.seen
                    .lock()
                    .expect("fake")
                    .push((system.to_string(), msgs.to_vec()));
                let next = f.replies.lock().expect("fake").pop_front();
                let full = next.unwrap_or(Err(LlmError::Failed("no reply".into())))?;
                let mut filter = StreamFilter::default();
                for ch in full.chars() {
                    if let Some(out) = filter.push(&ch.to_string()) {
                        let _ = tx.send(out);
                    }
                }
                if let Some(out) = filter.finish() {
                    let _ = tx.send(out);
                }
                Ok(full)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_practice::persona;

    #[test]
    fn 印の行は画面に流さない() {
        let mut f = StreamFilter::default();
        let mut shown = String::new();
        let text = "ええ、2名です。\n###{\"asked_slots\":[\"hiring_3m\"],\"end\":false}";
        for ch in text.chars() {
            if let Some(o) = f.push(&ch.to_string()) {
                shown.push_str(&o);
            }
        }
        if let Some(o) = f.finish() {
            shown.push_str(&o);
        }
        assert_eq!(shown.trim_end(), "ええ、2名です。");
        assert!(!shown.contains('#'));
    }

    #[test]
    fn 印が無い返事は最後まで出す() {
        let mut f = StreamFilter::default();
        let mut shown = String::new();
        for part in ["はい、", "どうぞ##"] {
            if let Some(o) = f.push(part) {
                shown.push_str(&o);
            }
        }
        assert_eq!(shown, "はい、どうぞ");
        shown.push_str(&f.finish().unwrap());
        assert_eq!(shown, "はい、どうぞ##");
    }

    #[test]
    fn 返事と印を読む() {
        let r = parse_reply(
            "今は予算は取っていないんです\n###{\"asked_slots\":[\"budget\",\"nope\",\"budget\"],\"is_question\":true,\"mood\":\"irritated\",\"end\":true}",
        );
        assert_eq!(r.text, "今は予算は取っていないんです");
        assert_eq!(r.asked_slots, ["budget"]);
        assert!(r.is_question && r.irritated && r.end);
        let r = parse_reply("はい、どうぞ");
        assert_eq!(r.text, "はい、どうぞ");
        assert!(r.asked_slots.is_empty() && !r.is_question && !r.end);
    }

    #[test]
    fn 要求の本文は指定どおりでサンプリング引数を含まない() {
        let msgs = vec![ChatMsg {
            role: "user",
            content: "こんにちは".into(),
        }];
        let b = AnthropicClient::request_body("claude-sonnet-5-5", "SYS", &msgs);
        assert_eq!(b["model"], "claude-sonnet-5-5");
        assert_eq!(b["stream"], true);
        assert_eq!(b["thinking"]["type"], "between_tools");
        assert_eq!(b["output_config"]["effort"], "low");
        assert_eq!(b["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(b["system"][0]["text"], "SYS");
        for k in ["temperature", "top_p", "top_k"] {
            assert!(b.get(k).is_none(), "{k}");
        }
        assert_eq!(b["messages"][0]["content"], "こんにちは");
    }

    #[test]
    fn システムプロンプトは型の答えを含み嫌がる段階はメモで渡す() {
        let p = persona::find("budget_01").unwrap();
        let s = build_system(p);
        assert!(s.contains("介護職を1名"));
        assert!(s.contains("月額定額の内容や金額感を聞いたあと"));
        let m = progress_memo(9, p, true);
        assert!(m.contains("9問") && m.contains("伝えられた") && m.contains("嫌がり始める"));
        assert!(progress_memo(12, p, false).contains("会話を閉じる"));
        assert!(progress_memo(2, p, false).contains("嫌がる段階: なし"));
    }
}
