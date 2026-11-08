//! 文字起こしを `checks.json` の項目で判定する (MiniMax、OpenAI 互換)。
//! プロンプトは試作 (`scripts/call_advice/call_advice.py` の `main_prompt` / `hearing_prompt`) と同じ文言。
//! 呼び出しは接触した電話 1 本につき 1 回 (今の判定) + 日程が決まった電話だけもう 1 回 (ヒアリング)。
//! モデルには判定と根拠の発言番号だけを書かせ、引用・助言・要約はコードで作る (headless-crm-design §22)。
//! 呼び出し部分は `Judge` / `Completer` に分けてあり、テストでは偽物に差し替える。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::checks::{Check, Checks};
use super::lang::{find_foreign, FALLBACK_TEXT};
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
    /// 先方の答え (`answers` のある項目だけ。選択肢に無い値は残さない)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// 営業がその項目を質問したか (`answers` のある項目だけ)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<bool>,
    /// 相応しい答えが先方から得られたか。営業が聞かずに先方が話した場合も true (`answers` のある項目だけ)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<bool>,
}

/// ヒアリングの項目 (`answers` のある項目) を「できた」にする基準。相応しい答えが得られたら完了 (2026-11-02 ユーザー決定)。
/// 判定では asked と answered の両方を残すので、基準を変えても判定し直さなくてよい
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HearingDone {
    /// 営業が質問したら完了
    Asked,
    /// 相応しい答えが得られたら完了
    Answered,
}

pub const HEARING_DONE: HearingDone = HearingDone::Answered;

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

    /// ヒアリングだけ (呼び出し 1 回)。ヒアリングの項目を checks の順で返す。
    /// ヒアリングを入れる前に判定した「日程が決まった」電話に、後から付けるために使う。
    /// `call_date` は電話をした日 (JST の YYYY-MM-DD)。入社時期の期間をこの日から数えさせる
    fn hearing(
        &self,
        checks: &Checks,
        turns: &[TalkTurn],
        call_date: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<StoredItem>, JudgeError>> + Send;
}

// ---------------------------------------------------------------------------
// プロンプト (試作 call_advice.py の main_prompt / hearing_prompt と同じ文言)
// ---------------------------------------------------------------------------

/// ヒアリングの項目の場面。日程が決まった電話だけ、別の呼び出しで判定する
pub const HEARING_STAGE: &str = "日程が決まった後";
/// ヒアリングを呼ぶ outcome
pub const SCHEDULED: &str = "日程が決まった";
/// ヒアリングの呼び出しは日程が決まった電話だけなので、該当なしがこれだけの項目はプロンプトに書かない
const NA_ONLY_UNSCHEDULED: &str = "日程が決まらなかった電話";
/// ヒアリングの呼び出しが失敗したときに、ヒアリングの項目に残すメモ
pub const HEARING_FAILED_NOTE: &str = "ヒアリングの判定を取れませんでした";
/// アポの記録が無い電話のヒアリングの項目に残すメモ (HubSpot のアポ日確定で対象を決める。`apo_link`)
pub const NO_APO_NOTE: &str = "アポの記録が無い電話のため対象外";
/// 応答に項目が無かったときのメモ
const MISSING_NOTE: &str = "判定の応答にこの項目がありませんでした";

pub fn is_hearing(c: &Check) -> bool {
    c.stage == HEARING_STAGE
}

/// 両方のプロンプトに入れる決まり (引用を書かせない・日本語の決まり・崩れの例)
const COMMON: &str = "- 引用は書かない。根拠は evidence の発言番号だけで示す(引用はこちらで文字起こしから写す)。
- 迷う項目は長く考えず、文字起こしから読み取れる範囲で簡単に決める。考え直しは考える部分で済ませ、JSON の中に書かない。

日本語の決まり:
- JSON の中の文は、日本の職場で使う自然な日本語だけで書く。中国語・韓国語・英語の単語、ローマ字、記号の羅列を混ぜない。
- 文字起こしの言葉、金額、月、人名は元の表記のまま書く。
- 書く前に、他の言語が混ざっていないか、途中で別の言葉にすり替わっていないかを確かめる。

悪い出力の例と正しい形:
悪い:「16時がetinyms的理由で難しConsumersになった後」
正しい:「16時が難しいと言われた後」
悪い:「ちょっと今考えてない-tradnego esque?auen? No! Wait」
正しい: (引用は書かず evidence に発言番号だけを入れる)
悪い:「名乗りorbitalでも伊藤さんと名前で呼んでいる」
正しい:「名乗りでも伊藤さんと名前で呼んでいる」";

fn transcript(turns: &[TalkTurn]) -> String {
    turns
        .iter()
        .map(|t| {
            format!(
                "{}. {}: {}",
                t.n,
                if t.speaker == "rep" { "営" } else { "先" },
                t.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 今の判定 (ヒアリング以外の項目)。接触した電話 1 本につき 1 回
pub fn build_prompt(checks: &Checks, turns: &[TalkTurn]) -> String {
    let items: Vec<String> = checks
        .checks
        .iter()
        .filter(|c| !is_hearing(c))
        .map(|c| {
            format!(
                "- id: {}\n  項目: {}\n  判定: {}\n  該当なし: {}",
                c.id, c.title, c.rule, c.na_when
            )
        })
        .collect();
    format!(
        r#"あなたは新規営業のアポ取り電話を評価する担当です。下の文字起こし(営=営業、先=先方。番号は発言の順番)を、項目ごとに判定してください。

決まり:
- アポが決まった後の会話(日時の復唱、Zoom・メールアドレスの確認など)は判定に使わない。決まったかどうかは outcome に書くだけにする。
- 判定は「できた」「できていない」「該当なし」のどれか。文字起こしが崩れていて判断できないときは「該当なし」にする。
- evidence には判定の根拠になった発言の番号を入れる。
{COMMON}

項目:
{items}

文字起こし:
{tr}

次の JSON だけを返してください:
{{"reached_person": true/false (担当者と話せたか),
 "reception": true/false (受付を通したか),
 "first_date_turn": 営業が最初に具体的な日付を出した発言の番号 または null,
 "outcome": "日程が決まった" / "決まらなかった" / "担当者不在" のどれか,
 "checks": [{{"id": "...", "status": "...", "evidence": [番号]}}]}}"#,
        items = items.join("\n"),
        tr = transcript(turns),
    )
}

/// ヒアリングの判定 (場面「日程が決まった後」の項目)。今の判定で日程が決まった電話だけ、もう 1 回。
/// `call_date` (電話をした日) があれば決まりに書き、入社時期の期間をその日から数えさせる
pub fn build_hearing_prompt(
    checks: &Checks,
    turns: &[TalkTurn],
    call_date: Option<&str>,
) -> String {
    let items: Vec<String> = checks
        .checks
        .iter()
        .filter(|c| is_hearing(c))
        .map(|c| {
            let mut line = format!(
                "- id: {}\n  項目: {}\n  判定: {}\n  先方の答え: {}",
                c.id,
                c.title,
                c.rule,
                c.answers.join(" / ")
            );
            if c.na_when != NA_ONLY_UNSCHEDULED {
                line.push_str(&format!("\n  該当なし: {}", c.na_when));
            }
            line
        })
        .collect();
    format!(
        r#"あなたは新規営業のアポ取り電話で、日程が決まった電話のヒアリングを確かめる担当です。下の文字起こし(営=営業、先=先方。番号は発言の順番)を、項目ごとに確かめてください。

決まり:
- 日程が決まる前も含めた電話全体で見る。前段で先方が話していれば答えが得られたとする。
- asked: 営業がその項目を質問したか (true/false)
- answered: 判定に書いた「相応しい答え」が先方から得られたか (true/false)。営業が聞かずに先方が自分から話した場合も true
- answer: 先方の答えを「先方の答え」の中から1つ。分からなければ「不明」
- status は「対象」。該当なしに当たるとき、または文字起こしが崩れて判断できないときだけ「該当なし」
- evidence は根拠の発言の番号。
{COMMON}
- detail: 具体的な金額や月が出たときだけ、その金額や月を書く(例「月20万円」「来年4月」)。無ければ空。{date}

項目:
{items}

文字起こし:
{tr}

次の JSON だけを返してください:
{{"checks": [{{"id": "...", "status": "...", "asked": true/false, "answered": true/false, "answer": "...", "evidence": [番号], "detail": "..."}}]}}"#,
        items = items.join("\n"),
        tr = transcript(turns),
        date = call_date.map(call_date_rule).unwrap_or_default(),
    )
}

/// 電話をした日の決まり (ヒアリングのプロンプトの決まりの最後の行)
fn call_date_rule(date: &str) -> String {
    format!("\n- この電話をした日は {date}。入社時期などの期間はこの日から数える(例: 11月の電話で「3月まで」「4月から」は4か月以上)。")
}

/// 電話の時刻 (エポックミリ秒) を JST の日付 YYYY-MM-DD にする
pub fn jst_date(call_ms: i64) -> Option<String> {
    let jst = chrono::FixedOffset::east_opt(9 * 3600)?;
    chrono::DateTime::from_timestamp_millis(call_ms)
        .map(|t| t.with_timezone(&jst).format("%Y-%m-%d").to_string())
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

fn na_item(c: &Check, note: Option<&str>) -> StoredItem {
    StoredItem {
        id: c.id.clone(),
        status: "na".into(),
        evidence: vec![],
        quote: None,
        advice: None,
        note: note.map(str::to_string),
        answer: None,
        asked: None,
        answered: None,
    }
}

fn evidence_of(r: &Value) -> Vec<u32> {
    match r.get("evidence") {
        Some(Value::Array(a)) => a.iter().filter_map(number_of).collect(),
        Some(v) => number_of(v).into_iter().collect(),
        None => vec![],
    }
}

/// 引用はモデルに書かせず、evidence の番号の発言を文字起こしから写す。
/// 複数は「／」でつなぎ、合計 `QUOTE_MAX_CHARS` 字で切る。文字起こしに無い番号は無視する
pub fn quote_from(turns: &[TalkTurn], evidence: &[u32]) -> Option<String> {
    let mut seen = Vec::new();
    let parts: Vec<&str> = evidence
        .iter()
        .filter(|n| {
            let first = !seen.contains(*n);
            seen.push(**n);
            first
        })
        .filter_map(|n| turns.iter().find(|t| t.n == *n))
        .map(|t| t.text.trim())
        .filter(|t| !t.is_empty())
        .collect();
    (!parts.is_empty()).then(|| trunc(&parts.join("／"), QUOTE_MAX_CHARS))
}

/// できていない項目の助言は checks.json の決まった言い方
fn advice_for(c: &Check, status: &str) -> Option<String> {
    (status == "not_done" && !c.advice.trim().is_empty()).then(|| c.advice.clone())
}

/// 今の判定の 1 項目 (できた / できていない / 該当なし)
fn convert_item(c: &Check, raw: Option<&Value>, turns: &[TalkTurn]) -> StoredItem {
    let Some(r) = raw else {
        return na_item(c, Some(MISSING_NOTE));
    };
    let status = status_from_ja(r.get("status").and_then(Value::as_str).unwrap_or(""));
    let evidence = evidence_of(r);
    StoredItem {
        id: c.id.clone(),
        status: status.to_string(),
        quote: quote_from(turns, &evidence),
        evidence,
        advice: advice_for(c, status),
        note: None,
        answer: None,
        asked: None,
        answered: None,
    }
}

/// ヒアリングの 1 項目 (status は「対象」/「該当なし」。できたかは `HEARING_DONE` の基準で決める)
fn convert_hearing_item(c: &Check, raw: Option<&Value>, turns: &[TalkTurn]) -> StoredItem {
    let Some(r) = raw else {
        return na_item(c, Some(MISSING_NOTE));
    };
    if r.get("status").and_then(Value::as_str).map(str::trim) == Some("該当なし") {
        return na_item(c, None);
    }
    let (asked, answered) = (bool_of(r, "asked"), bool_of(r, "answered"));
    let done = match HEARING_DONE {
        HearingDone::Asked => asked,
        HearingDone::Answered => answered,
    };
    let status = match done {
        Some(true) => "done",
        Some(false) => "not_done",
        // 読めなければ数えない
        None => return na_item(c, None),
    };
    let evidence = evidence_of(r);
    // detail (具体的な金額や月) はモデルが書く唯一の文。日本語以外が混ざっていたら捨てる (判定し直さない)
    let src = source_text(turns, &[]);
    StoredItem {
        id: c.id.clone(),
        status: status.to_string(),
        quote: quote_from(turns, &evidence),
        evidence,
        advice: advice_for(c, status),
        note: text_of(r, "detail").filter(|d| find_foreign(d, &src).is_none()),
        answer: text_of(r, "answer").filter(|a| c.answers.iter().any(|x| x == a)),
        asked,
        answered,
    }
}

fn find_in<'a>(v: &'a Value, id: &str) -> Option<&'a Value> {
    v.get("checks")
        .and_then(Value::as_array)?
        .iter()
        .find(|x| x.get("id").and_then(Value::as_str) == Some(id))
}

/// 今の判定の JSON → 保存する形。checks の順に並べ、欠けた項目は na にする。
/// ヒアリングの項目はここでは na (メモなし) で置き、日程が決まった電話だけ `apply_hearing` で埋める。
/// 要約は `summary_of` で別に作る
pub fn convert_response(checks: &Checks, v: &Value, turns: &[TalkTurn]) -> JudgeOutput {
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
            .map(|c| {
                if is_hearing(c) {
                    na_item(c, None)
                } else {
                    convert_item(c, find_in(v, &c.id), turns)
                }
            })
            .collect(),
        summary: None,
    }
}

/// ヒアリングの JSON で、items のヒアリングの項目を置き換える
/// ヒアリングの JSON で、items のヒアリングの項目を置き換える
pub fn apply_hearing(checks: &Checks, v: &Value, turns: &[TalkTurn], items: &mut [StoredItem]) {
    for c in checks.checks.iter().filter(|c| is_hearing(c)) {
        if let Some(i) = items.iter_mut().find(|i| i.id == c.id) {
            *i = convert_hearing_item(c, find_in(v, &c.id), turns);
        }
    }
}

/// ヒアリングの項目をすべて該当なし (メモ付き) にした一覧 (checks の順)
pub fn hearing_na(checks: &Checks, note: &str) -> Vec<StoredItem> {
    checks
        .checks
        .iter()
        .filter(|c| is_hearing(c))
        .map(|c| na_item(c, Some(note)))
        .collect()
}

/// 要約: checks の順で最初の「できていない」項目の助言 (無ければ None)
pub fn summary_of(items: &[StoredItem]) -> Option<String> {
    items
        .iter()
        .find(|i| i.status == "not_done")
        .and_then(|i| i.advice.clone())
}

// ---------------------------------------------------------------------------
// 文章の言語の検査 (日本語以外が混ざった文章は画面に出さない。ADR-033 追記)
// 今の判定ではモデルが文を書かないので、新しく判定する分で検査するのはヒアリングの detail だけ。
// 下の関数は、前のプロンプトで判定した保存済みの行を探して直す worker のために残している
// ---------------------------------------------------------------------------

/// 日本語の文章を最初から書き直させる最大回数 (最初の 1 回に足す)。アポの判定 (`apo_judge`) が使う。
/// トークの振り返りは文をほぼ書かせないので、やり直さない (ヒアリングの detail は混ざったら捨てる)
pub const LANG_RETRIES: usize = 2;

/// 英単語の許可に使う元の文字 (文字起こしと引用)
pub fn source_text(turns: &[TalkTurn], items: &[StoredItem]) -> String {
    let mut s: String = turns
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for q in items.iter().filter_map(|i| i.quote.as_deref()) {
        s.push('\n');
        s.push_str(q);
    }
    s
}

/// 助言・メモ・まとめに日本語以外が混ざっているか (引用は検査しない)
pub fn texts_foreign(items: &[StoredItem], summary: Option<&str>, source: &str) -> bool {
    items.iter().any(|i| {
        i.advice
            .as_deref()
            .is_some_and(|t| find_foreign(t, source).is_some())
            || i.note
                .as_deref()
                .is_some_and(|t| find_foreign(t, source).is_some())
    }) || summary.is_some_and(|t| find_foreign(t, source).is_some())
}

/// 混ざった文章を取り除く。助言は消して、メモに「作れませんでした」を入れる。まとめは置き換える。
/// 判定の結果 (できた / できていない) は残す
pub fn sanitize_texts(items: &mut [StoredItem], summary: &mut Option<String>, source: &str) {
    for i in items {
        let bad_advice = i
            .advice
            .as_deref()
            .is_some_and(|t| find_foreign(t, source).is_some());
        let bad_note = i
            .note
            .as_deref()
            .is_some_and(|t| find_foreign(t, source).is_some());
        if bad_advice {
            i.advice = None;
        }
        if bad_note || (bad_advice && i.note.is_none()) {
            i.note = Some(FALLBACK_TEXT.to_string());
        }
    }
    if summary
        .as_deref()
        .is_some_and(|t| find_foreign(t, source).is_some())
    {
        *summary = Some(FALLBACK_TEXT.to_string());
    }
}

/// プロンプトを送って JSON を返すもの (テストでは偽物に差し替える)
pub trait Completer: Send + Sync {
    fn complete(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<Value, JudgeError>> + Send;
}

/// 今の判定だけ (呼び出し 1 回。ヒアリングの項目は該当なし)。電話の練習 (`call_practice`) はこちらを使う
pub async fn judge_main_via<C: Completer>(
    c: &C,
    checks: &Checks,
    turns: &[TalkTurn],
) -> Result<JudgeOutput, JudgeError> {
    let v = c.complete(&build_prompt(checks, turns)).await?;
    let mut out = convert_response(checks, &v, turns);
    out.summary = summary_of(&out.items);
    Ok(out)
}

/// 1 本の判定。MiniMax の呼び出しは、今の判定 1 回 + 日程が決まった電話だけヒアリング 1 回。
/// - 今の判定が失敗したらエラー (ヒアリングは呼ばない。行ごと後で再試行)
/// - ヒアリングだけが失敗したら、ヒアリングの項目を該当なし (メモ `HEARING_FAILED_NOTE`) にして判定済みにする。
///   行ごと再試行にすると、成功した今の判定までもう一度呼ぶことになるため (headless-crm-design §22)
pub async fn judge_via<C: Completer>(
    c: &C,
    checks: &Checks,
    turns: &[TalkTurn],
    call_date: Option<&str>,
) -> Result<JudgeOutput, JudgeError> {
    let mut out = judge_main_via(c, checks, turns).await?;
    if out.outcome.as_deref() == Some(SCHEDULED) && checks.checks.iter().any(is_hearing) {
        match c
            .complete(&build_hearing_prompt(checks, turns, call_date))
            .await
        {
            Ok(h) => apply_hearing(checks, &h, turns, &mut out.items),
            Err(e) => {
                // 理由は短い記号だけ (http 429・json など)。本文・鍵は含まない
                tracing::info!(reason = ?e, "talk review: hearing judge failed");
                for c in checks.checks.iter().filter(|c| is_hearing(c)) {
                    if let Some(i) = out.items.iter_mut().find(|i| i.id == c.id) {
                        *i = na_item(c, Some(HEARING_FAILED_NOTE));
                    }
                }
            }
        }
    }
    out.summary = summary_of(&out.items);
    Ok(out)
}

/// ヒアリングだけの判定 (`Judge::hearing` の中身)
pub async fn hearing_via<C: Completer>(
    c: &C,
    checks: &Checks,
    turns: &[TalkTurn],
    call_date: Option<&str>,
) -> Result<Vec<StoredItem>, JudgeError> {
    let v = c
        .complete(&build_hearing_prompt(checks, turns, call_date))
        .await?;
    let mut items: Vec<StoredItem> = checks
        .checks
        .iter()
        .filter(|c| is_hearing(c))
        .map(|c| na_item(c, None))
        .collect();
    apply_hearing(checks, &v, turns, &mut items);
    Ok(items)
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

    /// MiniMax-M3 は temperature・top_p を受け付けても無視し、常に 1 / 0.95 で動く (公式ドキュメント)。
    /// temperature 0 は送っているが、判定の揺れは抑えられない前提で扱う
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

impl Completer for MiniMaxJudge {
    async fn complete(&self, prompt: &str) -> Result<Value, JudgeError> {
        self.complete_json(prompt).await
    }
}

impl Judge for MiniMaxJudge {
    /// 今の判定だけ。ヒアリングは、HubSpot のアポの記録と突き合わせてから worker が別に付ける (`apo_link`)
    async fn judge(&self, checks: &Checks, turns: &[TalkTurn]) -> Result<JudgeOutput, JudgeError> {
        judge_main_via(self, checks, turns).await
    }

    async fn hearing(
        &self,
        checks: &Checks,
        turns: &[TalkTurn],
        call_date: Option<&str>,
    ) -> Result<Vec<StoredItem>, JudgeError> {
        hearing_via(self, checks, turns, call_date).await
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

    fn tt(n: u32, rep: bool, text: &str) -> TalkTurn {
        TalkTurn {
            n,
            speaker: if rep { "rep" } else { "customer" }.into(),
            text: text.into(),
        }
    }

    /// 架空の 9 発言 (日程が決まった電話)
    fn turns9() -> Vec<TalkTurn> {
        vec![
            tt(1, true, "お世話になっております、採用支援のFACの佐藤です。"),
            tt(2, false, "はい。"),
            tt(
                3,
                true,
                "前回6月にお電話したんですけど、タイミングが合わなかったので改めました。",
            ),
            tt(4, false, "ああ、そうでしたね。"),
            tt(5, true, "来週の水曜か木曜あたりはいかがですか。"),
            tt(6, false, "木曜の14時なら大丈夫です。"),
            tt(7, true, "ご予算は月にどのくらいを見ていらっしゃいますか。"),
            tt(8, false, "月20万円くらいですね。"),
            tt(
                9,
                true,
                "ありがとうございます。では木曜14時でお願いします。",
            ),
        ]
    }

    fn check<'a>(c: &'a Checks, id: &str) -> &'a Check {
        c.checks.iter().find(|x| x.id == id).unwrap()
    }
    fn item<'a>(o: &'a [StoredItem], id: &str) -> &'a StoredItem {
        o.iter().find(|i| i.id == id).unwrap()
    }

    #[test]
    fn quote_is_copied_from_the_evidence_turns() {
        let t = turns9();
        assert_eq!(
            quote_from(&t, &[5, 6]).as_deref(),
            Some("来週の水曜か木曜あたりはいかがですか。／木曜の14時なら大丈夫です。")
        );
        // 文字起こしに無い番号 (0・99) は無視し、重なった番号は 1 回だけ
        assert_eq!(
            quote_from(&t, &[0, 8, 99, 8]).as_deref(),
            Some("月20万円くらいですね。")
        );
        assert_eq!(quote_from(&t, &[42]), None);
        assert_eq!(quote_from(&t, &[]), None);
        // 合計 200 字で切る
        let long = vec![
            tt(1, true, &"あ".repeat(150)),
            tt(2, false, &"い".repeat(150)),
        ];
        let q = quote_from(&long, &[1, 2]).unwrap();
        assert_eq!(q.chars().count(), 200);
        assert!(q.starts_with(&"あ".repeat(150)));
        assert_eq!(q.chars().nth(150), Some('／'));
    }

    #[test]
    fn converts_model_response_in_checks_order() {
        let c = &loaded().checks;
        let t = turns9();
        let v = json!({
            "reached_person": true, "reception": "false", "first_date_turn": 5,
            "outcome": "決まらなかった",
            "checks": [
                {"id": "early_date", "status": "できていない", "evidence": [5, "6", 77]},
                {"id": "reception_name", "status": "該当なし", "evidence": []},
                {"id": "busy_reoffer", "status": "できた", "evidence": [3],
                 "quote": "書いても使わない", "advice": "書いても使わない", "note": "書いても使わない"}
            ]
        });
        let o = convert_response(c, &v, &t);
        assert_eq!(o.items.len(), 15);
        assert_eq!(o.items[0].id, "reception_name");
        assert_eq!(
            (o.items[0].status.as_str(), o.items[0].quote.as_deref()),
            ("na", None)
        );
        let early = &o.items[2];
        assert_eq!(early.id, "early_date");
        assert_eq!(early.status, "not_done");
        assert_eq!(early.evidence, vec![5, 6, 77]);
        assert_eq!(
            early.quote.as_deref(),
            Some("来週の水曜か木曜あたりはいかがですか。／木曜の14時なら大丈夫です。")
        );
        // 助言は checks.json の決まった言い方
        assert_eq!(
            early.advice.as_deref(),
            Some(check(c, "early_date").advice.as_str())
        );
        assert!(early
            .advice
            .as_deref()
            .unwrap()
            .starts_with("前回のことに一言触れ"));
        assert_eq!(early.note, None);
        let busy = item(&o.items, "busy_reoffer");
        assert_eq!(busy.status, "done");
        // モデルが quote / advice / note を書いても使わない
        assert_eq!(
            busy.quote.as_deref(),
            Some("前回6月にお電話したんですけど、タイミングが合わなかったので改めました。")
        );
        assert_eq!((busy.advice.as_deref(), busy.note.as_deref()), (None, None));
        // 応答に無い項目は na とメモ
        let k = item(&o.items, "kizonkan");
        assert_eq!(
            (k.status.as_str(), k.note.as_deref()),
            ("na", Some(MISSING_NOTE))
        );
        // ヒアリングの項目はここでは na (メモなし)
        let b = item(&o.items, "aq_budget");
        assert_eq!((b.status.as_str(), b.note.as_deref()), ("na", None));
        assert_eq!(o.reached_person, Some(true));
        assert_eq!(o.reception, Some(false));
        assert_eq!(o.first_date_turn, Some(5));
        assert_eq!(o.outcome.as_deref(), Some("決まらなかった"));
        assert_eq!(o.summary, None, "要約は summary_of で作る");
        // 要約は checks の順で最初の「できていない」項目の助言
        assert_eq!(
            summary_of(&o.items).as_deref(),
            Some(check(c, "early_date").advice.as_str())
        );
        assert_eq!(summary_of(&[]), None);
    }

    #[test]
    fn summary_is_the_first_not_done_in_checks_order() {
        let c = &loaded().checks;
        let v = json!({"outcome": "決まらなかった", "checks": [
            {"id": "busy_concrete_day", "status": "できていない", "evidence": []},
            {"id": "no_demand_question", "status": "できていない", "evidence": []},
            {"id": "reception_name", "status": "できた", "evidence": []}
        ]});
        let o = convert_response(c, &v, &turns9());
        // 応答の順ではなく checks の順 (no_demand_question は 2 番目、busy_concrete_day は 8 番目)
        assert_eq!(
            summary_of(&o.items).as_deref(),
            Some(check(c, "no_demand_question").advice.as_str())
        );
    }

    #[test]
    fn unknown_outcome_is_dropped() {
        let c = &loaded().checks;
        let v = json!({"outcome": "不明", "checks": [{"id": "early_date", "status": "できた"}]});
        let o = convert_response(c, &v, &turns9());
        assert_eq!(o.outcome, None);
    }

    /// 偽の MiniMax。届いたプロンプトを数え、順に用意した応答を返す (尽きたら失敗)
    struct Scripted(
        std::sync::Mutex<Vec<Result<Value, JudgeError>>>,
        std::sync::Mutex<Vec<String>>,
    );
    impl Scripted {
        fn new(r: Vec<Result<Value, JudgeError>>) -> Self {
            Self(std::sync::Mutex::new(r), Default::default())
        }
        fn prompts(&self) -> Vec<String> {
            self.1.lock().unwrap().clone()
        }
    }
    impl Completer for Scripted {
        async fn complete(&self, prompt: &str) -> Result<Value, JudgeError> {
            self.1.lock().unwrap().push(prompt.to_string());
            let mut q = self.0.lock().unwrap();
            if q.is_empty() {
                Err(JudgeError::Transient("empty".into()))
            } else {
                q.remove(0)
            }
        }
    }
    fn main_resp(outcome: &str) -> Value {
        json!({"reached_person": true, "reception": false, "first_date_turn": 5, "outcome": outcome,
               "checks": [{"id": "early_date", "status": "できた", "evidence": [5]},
                          {"id": "kizonkan", "status": "できていない", "evidence": [3]}]})
    }
    fn hearing_resp() -> Value {
        json!({"checks": [
            {"id": "aq_budget", "status": "対象", "asked": true, "answered": true, "answer": "具体額",
             "evidence": [7, 8], "detail": "月20万円"},
            {"id": "aq_timing", "status": "対象", "asked": false, "answered": false, "answer": "不明",
             "evidence": [], "detail": ""},
            {"id": "aq_job", "status": "対象", "asked": true, "answered": true, "answer": "建設",
             "evidence": [99], "detail": "施工管理orbital"},
            {"id": "aq_attend", "status": "該当なし", "asked": true, "answered": true, "answer": "判断者が同席する"}
        ]})
    }

    #[tokio::test]
    async fn hearing_is_called_only_when_the_date_was_set() {
        let c = &loaded().checks;
        let s = Scripted::new(vec![Ok(main_resp("日程が決まった")), Ok(hearing_resp())]);
        let o = judge_via(&s, c, &turns9(), None).await.unwrap();
        let p = s.prompts();
        assert_eq!(p.len(), 2, "今の判定 1 回 + ヒアリング 1 回");
        assert!(p[0].starts_with("あなたは新規営業のアポ取り電話を評価する担当です。"));
        assert!(
            p[1].starts_with("あなたは新規営業のアポ取り電話で、日程が決まった電話のヒアリング")
        );
        let b = item(&o.items, "aq_budget");
        assert_eq!(
            (b.status.as_str(), b.asked, b.answered, b.answer.as_deref()),
            ("done", Some(true), Some(true), Some("具体額"))
        );
        assert_eq!(
            b.quote.as_deref(),
            Some("ご予算は月にどのくらいを見ていらっしゃいますか。／月20万円くらいですね。")
        );
        assert_eq!(
            b.note.as_deref(),
            Some("月20万円"),
            "detail は note に入れる"
        );
        assert_eq!(b.advice, None);
        let t = item(&o.items, "aq_timing");
        assert_eq!(
            (t.status.as_str(), t.answer.as_deref(), t.note.as_deref()),
            ("not_done", Some("不明"), None)
        );
        assert_eq!(
            t.advice.as_deref(),
            Some(check(c, "aq_timing").advice.as_str())
        );
        // 選択肢に無い答えは残さない。範囲外の番号は引用にしない。日本語以外が混ざった detail は捨てる
        let j = item(&o.items, "aq_job");
        assert_eq!(
            (
                j.status.as_str(),
                j.answer.as_deref(),
                j.quote.as_deref(),
                j.note.as_deref()
            ),
            ("done", None, None, None)
        );
        let a = item(&o.items, "aq_attend");
        assert_eq!(
            (a.status.as_str(), a.asked, a.answer.as_deref()),
            ("na", None, None)
        );
        // 応答に無いヒアリングの項目は na とメモ
        assert_eq!(
            item(&o.items, "aq_reason").note.as_deref(),
            Some(MISSING_NOTE)
        );
        // 要約は checks の順で最初の「できていない」= kizonkan (ヒアリングより前)
        assert_eq!(
            o.summary.as_deref(),
            Some(check(c, "kizonkan").advice.as_str())
        );
    }

    #[tokio::test]
    async fn hearing_only_returns_the_hearing_items_in_order() {
        let c = &loaded().checks;
        let s = Scripted::new(vec![Ok(hearing_resp())]);
        let h = hearing_via(&s, c, &turns9(), None).await.unwrap();
        assert!(!s.prompts()[0].contains("call_kind"));
        assert_eq!(
            h.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            c.checks
                .iter()
                .filter(|c| is_hearing(c))
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(item(&h, "aq_budget").status, "done");
        assert!(hearing_na(c, NO_APO_NOTE)
            .iter()
            .all(|i| i.status == "na" && i.note.as_deref() == Some(NO_APO_NOTE)));
    }

    #[tokio::test]
    async fn hearing_is_not_called_when_the_date_was_not_set() {
        let c = &loaded().checks;
        for outcome in ["決まらなかった", "担当者不在", "読めない値"] {
            let s = Scripted::new(vec![Ok(main_resp(outcome)), Ok(hearing_resp())]);
            let o = judge_via(&s, c, &turns9(), None).await.unwrap();
            assert_eq!(s.prompts().len(), 1, "{outcome}: 今の判定の 1 回だけ");
            for i in o.items.iter().filter(|i| i.id.starts_with("aq_")) {
                assert_eq!((i.status.as_str(), i.note.as_deref()), ("na", None));
            }
        }
    }

    #[tokio::test]
    async fn main_failure_skips_hearing_and_hearing_failure_keeps_the_main_result() {
        let c = &loaded().checks;
        // 今の判定が失敗したらヒアリングは呼ばず、エラー (行ごと後で再試行)
        let s = Scripted::new(vec![
            Err(JudgeError::Transient("json".into())),
            Ok(hearing_resp()),
        ]);
        assert_eq!(
            judge_via(&s, c, &turns9(), None).await,
            Err(JudgeError::Transient("json".into()))
        );
        assert_eq!(s.prompts().len(), 1);
        // ヒアリングだけ失敗: 今の判定は残し、ヒアリングの項目は該当なし + メモで判定済み (呼び直さない)
        let s = Scripted::new(vec![
            Ok(main_resp("日程が決まった")),
            Err(JudgeError::Transient("http 429".into())),
        ]);
        let o = judge_via(&s, c, &turns9(), None).await.unwrap();
        assert_eq!(s.prompts().len(), 2);
        assert_eq!(item(&o.items, "early_date").status, "done");
        assert_eq!(item(&o.items, "kizonkan").status, "not_done");
        for i in o.items.iter().filter(|i| i.id.starts_with("aq_")) {
            assert_eq!(
                (i.status.as_str(), i.note.as_deref()),
                ("na", Some(HEARING_FAILED_NOTE))
            );
        }
        assert_eq!(o.outcome.as_deref(), Some("日程が決まった"));
    }

    #[test]
    fn prompts_follow_the_tested_wording() {
        let c = &loaded().checks;
        let turns = vec![tt(1, true, "もしもし"), tt(2, false, "はい")];
        let p = build_prompt(c, &turns);
        assert!(p.contains("- id: reception_name\n  項目: 受付で担当者を名前で呼ぶ\n  判定: "));
        assert!(!p.contains("場面:"));
        assert!(!p.contains("aq_"), "ヒアリングの項目は今の判定に入れない");
        assert!(p.contains("1. 営: もしもし\n2. 先: はい"));
        assert!(p.contains("- 引用は書かない。根拠は evidence の発言番号だけで示す"));
        assert!(p.contains(
            "\"checks\": [{\"id\": \"...\", \"status\": \"...\", \"evidence\": [番号]}]}"
        ));
        assert!(!p.contains("quote") && !p.contains("advice") && !p.contains("summary"));
        let h = build_hearing_prompt(c, &turns, None);
        assert_eq!(h.matches("- id: aq_").count(), 6);
        assert!(h.contains(
            "- id: aq_budget\n  項目: 予算\n  判定: 質問: 採用にかける予算を営業が聞いたか。"
        ));
        assert!(
            h.contains("  先方の答え: 具体額 / 無い / 都度確保 / 未定 / 不明\n- id: aq_decider")
        );
        // 該当なしは「日程が決まらなかった電話」以外の条件がある項目 (aq_attend) だけ書く
        assert_eq!(h.matches("  該当なし: ").count(), 1);
        assert!(h.contains("  該当なし: 日程が決まらなかった電話、または話している相手本人が判断する人だと分かった電話"));
        assert!(h.contains(
            "正しい:「名乗りでも伊藤さんと名前で呼んでいる」\n- detail: 具体的な金額や月"
        ));
        // 電話をした日を渡すと、決まりの最後に書き、期間をその日から数えさせる
        assert!(!h.contains("この電話をした日"));
        let d = build_hearing_prompt(c, &turns, Some("2026-11-05"));
        assert!(d.contains("無ければ空。\n- この電話をした日は 2026-11-05。入社時期などの期間はこの日から数える(例: 11月の電話で「3月まで」「4月から」は4か月以上)。\n\n項目:"));
        assert!(d.contains("期間は電話をした日から数える。"));
        // JST の日付 (UTC の 15 時は翌日)
        assert_eq!(jst_date(1793890800000).as_deref(), Some("2026-11-06"));
        let b = MiniMaxJudge::request_body("MiniMax-M3", "P");
        assert_eq!(b["temperature"], json!(0));
        assert_eq!(b["model"], json!("MiniMax-M3"));
    }

    /// Rust と Python の試作のプロンプトが同じ文言か (python3 があるときだけ確かめる)
    #[test]
    fn prompts_match_the_python_prototype() {
        let script = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/call_advice/call_advice.py"
        );
        let checks = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/crm/talk_review/checks.json"
        );
        let code = format!(
            "import json,sys,importlib.util\n\
             s=importlib.util.spec_from_file_location('ca','{script}');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)\n\
             c=json.load(open('{checks}',encoding='utf-8'))\n\
             t=[{{'n':1,'sp':'営','text':'もしもし'}},{{'n':2,'sp':'先','text':'はい'}}]\n\
             sys.stdout.write(json.dumps([m.main_prompt(c,t),m.hearing_prompt(c,t),m.hearing_prompt(c,t,'2026-11-05')],ensure_ascii=False))"
        );
        let Ok(out) = std::process::Command::new("python3")
            .args(["-I", "-c", &code])
            .output()
        else {
            eprintln!("python3 が無いので試作との照合を飛ばす");
            return;
        };
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let py: Vec<String> = serde_json::from_slice(&out.stdout).unwrap();
        let c = &loaded().checks;
        let turns = vec![tt(1, true, "もしもし"), tt(2, false, "はい")];
        assert_eq!(py[0], build_prompt(c, &turns));
        assert_eq!(py[1], build_hearing_prompt(c, &turns, None));
        assert_eq!(py[2], build_hearing_prompt(c, &turns, Some("2026-11-05")));
    }
}
