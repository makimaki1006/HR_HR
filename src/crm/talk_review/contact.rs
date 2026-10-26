//! 接触・日程切りの判定 (純粋な部分と、1 本の処理)。
//!
//! - 接触: 受付を越えて、営業が担当者または決裁者本人と話せた発信。日程を出さなくても接触に数える。
//! - 日程切り: 具体的な日時 (週・曜日・日付) を 1 回以上出した発信。出した回数も数える。
//!
//! 対象は Zoom の通話ログ (`phone/call_logs`) の発信のうち `Auto Recorded` (録音のあるつながった通話)。
//! つながっていない発信は判定せず、接触 0・日程切り 0 として分母 (発信数) にだけ入る。

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, Utc};
use serde_json::Value;

use super::judge::{bool_of, extract_json, number_of, JudgeError, MiniMaxJudge};
use super::store::{ts_text, ST_UNAVAILABLE};
use super::transcript::has_both_sides;
use super::types::{TalkPersonRow, TalkTurn};
use super::worker::{backoff, MAX_ATTEMPTS};
use super::zoom::{TranscriptSource, ZoomError};

pub const ST_PENDING: &str = "pending";
pub const ST_JUDGED: &str = "judged";
/// アポ取り以外の電話 (かけた時点の取引のステージが対象外)。判定せず、接触・日程切りにも数えない
pub const ST_OUT_OF_SCOPE: &str = "out_of_scope";

/// 録音が消えたとみなす通話の年齢
const GONE_AFTER: ChronoDuration = ChronoDuration::days(1);
/// 1 回の判定に渡す文字起こしの長さの上限 (文字数)。長い電話は前から
pub const MAX_PROMPT_CHARS: usize = 8000;
/// 1 本の電話で数える日程切りの回数の上限 (読み違いで大きな数が入るのを防ぐ)
pub const MAX_PROPOSALS: u32 = 20;

// ---------------------------------------------------------------------------
// 見つける (Zoom の通話ログ)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Zoom の通話ログ ID
    pub id: String,
    pub owner_id: String,
    /// UTC、`ts_text` の形
    pub call_at: String,
    /// 被呼番号の下 9 桁 (通話ログの `callee_number`)。HubSpot の Call と結び付けて取引のステージを見るのに使う
    pub callee9: Option<String>,
}

/// 通話ログのページから、JST の `day` の「つながった発信」(録音あり)。
/// 発信だけ・`id`/`call_id` で重複除外・JST の日に入るもの・`user_id` 必須・`result == "Auto Recorded"`。
/// Zoom ユーザー → メール → HubSpot の担当者に結び付き、`allowed` (対象チームの人) に入る人だけ。
pub fn discover(
    pages: &[Value],
    day: NaiveDate,
    user_email: &HashMap<String, String>,
    email_owner: &HashMap<String, String>,
    allowed: &HashSet<String>,
) -> Vec<Candidate> {
    let start = day.and_hms_opt(0, 0, 0).expect("midnight").and_utc() - ChronoDuration::hours(9);
    let end = start + ChronoDuration::days(1);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for page in pages {
        let Some(logs) = page.get("call_logs").and_then(Value::as_array) else {
            continue;
        };
        for log in logs {
            if log.get("direction").and_then(Value::as_str) != Some("outbound") {
                continue;
            }
            let text = |k: &str| log.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
            let Some(id) = text("id").or_else(|| text("call_id")) else {
                continue;
            };
            if !seen.insert(id.to_string()) {
                continue;
            }
            let Some(at) = text("date_time").or_else(|| text("start_time")) else {
                continue;
            };
            let Ok(when) = DateTime::parse_from_rfc3339(at) else {
                continue;
            };
            let when = when.with_timezone(&Utc);
            if when < start || when >= end {
                continue;
            }
            if text("result") != Some("Auto Recorded") {
                continue;
            }
            let Some(uid) = text("user_id") else {
                continue;
            };
            let Some(owner) = user_email.get(uid).and_then(|e| email_owner.get(e)) else {
                continue;
            };
            if !allowed.contains(owner) {
                continue;
            }
            out.push(Candidate {
                id: id.to_string(),
                owner_id: owner.clone(),
                call_at: ts_text(when),
                callee9: text("callee_number")
                    .or_else(|| text("callee_did_number"))
                    .and_then(super::zoom::normalize_number),
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 判定 (MiniMax)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactResult {
    pub contacted: bool,
    /// 具体的な日時を出した回数。接触していなければ 0
    pub date_proposals: u32,
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
        if tr.chars().count() + line.chars().count() > MAX_PROMPT_CHARS {
            break;
        }
        tr.push_str(&line);
    }
    format!(
        r#"新規営業のアポ取り電話の文字起こし(営=営業、先=先方)を読み、次の2点を判定してください。

- contacted: 受付や留守電ではなく、営業が担当者または決裁者(採用や経営の判断をする人)本人と話せたら true。日程を出していなくてもよい。受付だけで終わった・留守電・自動音声・担当者ではない人と少し話しただけ・名乗る前に切られた場合は false。
- date_proposals: 営業が具体的な日時(週・曜日・日付。「来週の水曜か木曜」「10月25日」など)を出して打診した回数。同じ打診の言い直しも1回ずつ数える。「お時間をいただけませんか」のような日付のない依頼は数えない。contacted が false なら 0。

文字起こし:
{tr}
次の JSON だけを返してください: {{"contacted": true/false, "date_proposals": 回数}}"#
    )
}

/// モデルの JSON → 結果。`contacted` が読めなければ None。回数は上限で切り、非接触なら 0
pub fn parse_result(v: &Value) -> Option<ContactResult> {
    let contacted = bool_of(v, "contacted")?;
    let n = v
        .get("date_proposals")
        .and_then(number_of)
        .unwrap_or(0)
        .min(MAX_PROPOSALS);
    Some(ContactResult {
        contacted,
        date_proposals: if contacted { n } else { 0 },
    })
}

pub trait ContactJudge: Send + Sync {
    fn judge(
        &self,
        turns: &[TalkTurn],
    ) -> impl std::future::Future<Output = Result<ContactResult, JudgeError>> + Send;
}

impl ContactJudge for MiniMaxJudge {
    async fn judge(&self, turns: &[TalkTurn]) -> Result<ContactResult, JudgeError> {
        let v = self.complete_json(&build_prompt(turns)).await?;
        parse_result(&v).ok_or_else(|| JudgeError::Transient("contact json".into()))
    }
}

/// 応答の文章から結果を取り出す (テスト用)
pub fn parse_text(text: &str) -> Option<ContactResult> {
    parse_result(&extract_json(text)?)
}

// ---------------------------------------------------------------------------
// 1 本の処理
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Judged(ContactResult),
    /// 後で再試行 (理由の短い記号)
    Retry(&'static str),
    Unavailable(&'static str),
}

pub async fn process_one<Z: TranscriptSource, J: ContactJudge>(
    zoom: &Z,
    judge: &J,
    id: &str,
    call_at: &str,
    now: DateTime<Utc>,
) -> Outcome {
    let turns = match zoom.fetch(id).await {
        Ok(t) => t,
        Err(ZoomError::NotReady) => return Outcome::Retry("transcript_not_ready"),
        Err(ZoomError::Untranscribable) => return Outcome::Unavailable("untranscribable"),
        Err(ZoomError::Gone) => {
            let old = DateTime::parse_from_rfc3339(call_at)
                .is_ok_and(|t| now - t.with_timezone(&Utc) >= GONE_AFTER);
            return if old {
                Outcome::Unavailable("recording_gone")
            } else {
                Outcome::Retry("recording_not_found_yet")
            };
        }
        Err(ZoomError::Transient(_)) => return Outcome::Retry("zoom_error"),
        Err(ZoomError::NotConfigured) => return Outcome::Retry("zoom_not_configured"),
    };
    if turns.is_empty() {
        return Outcome::Retry("transcript_empty");
    }
    // 営業の声しか無い通話 (留守電・自動音声) は接触ではない。営業の声が無いものは話者を分けられない
    if !has_both_sides(&turns) {
        return if turns.iter().any(|t| t.speaker == "rep") {
            Outcome::Judged(ContactResult {
                contacted: false,
                date_proposals: 0,
            })
        } else {
            Outcome::Unavailable("speakers_not_separated")
        };
    }
    match judge.judge(&turns).await {
        Ok(r) => Outcome::Judged(r),
        Err(JudgeError::Transient(m)) if m.starts_with("http 429") => {
            Outcome::Retry("judge_rate_limited")
        }
        Err(JudgeError::Transient(_)) => Outcome::Retry("judge_error"),
        Err(JudgeError::NotConfigured) => Outcome::Retry("judge_not_configured"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Judged(ContactResult, u32),
    Retry { attempts: u32, next: DateTime<Utc> },
    Final { status: &'static str, attempts: u32 },
}

pub fn decide(outcome: Outcome, prev_attempts: u32, now: DateTime<Utc>) -> Update {
    let attempts = prev_attempts + 1;
    match outcome {
        Outcome::Judged(r) => Update::Judged(r, attempts),
        Outcome::Unavailable(_) => Update::Final {
            status: ST_UNAVAILABLE,
            attempts,
        },
        Outcome::Retry(_) if attempts >= MAX_ATTEMPTS => Update::Final {
            status: ST_UNAVAILABLE,
            attempts,
        },
        Outcome::Retry(_) => Update::Retry {
            attempts,
            next: now + backoff(attempts),
        },
    }
}

// ---------------------------------------------------------------------------
// 集計 (人ごと)
// ---------------------------------------------------------------------------

/// 期間内の 1 人分の合計 (表 `call_contact_marks` の GROUP BY の結果)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContactAgg {
    pub contacts: u32,
    /// 日程を 1 回以上出した発信の本数
    pub proposal_calls: u32,
    /// 日程を出した回数の合計 (接触した発信の分)
    pub proposals: u32,
    /// 判定がまだ済んでいない、つながった発信の本数
    pub pending: u32,
    /// アポ取り以外として外した発信の本数 (率の分母から引く)
    pub out_of_scope: u32,
}

impl ContactAgg {
    pub fn add(&mut self, o: &ContactAgg) {
        self.contacts += o.contacts;
        self.proposal_calls += o.proposal_calls;
        self.proposals += o.proposals;
        self.pending += o.pending;
        self.out_of_scope += o.out_of_scope;
    }
}

fn pct(num: u32, den: Option<u32>) -> Option<f64> {
    den.filter(|d| *d > 0)
        .map(|d| f64::from(num) / f64::from(d) * 100.0)
}

/// 率の分母: アポ取りの発信だけ (発信数からアポ取り以外として外した本数を引く)。発信数の表示は営業KPIのまま
fn rate_den(row: &TalkPersonRow, a: &ContactAgg) -> Option<u32> {
    row.outbound.map(|o| o.saturating_sub(a.out_of_scope))
}

fn fill(row: &mut TalkPersonRow, a: &ContactAgg, den: Option<u32>) {
    row.contacts = Some(a.contacts);
    row.contact_rate_pct = pct(a.contacts, den);
    row.date_proposal_calls = Some(a.proposal_calls);
    row.date_proposal_rate_pct = pct(a.proposal_calls, den);
    row.proposals_per_contact =
        (a.contacts > 0).then(|| f64::from(a.proposals) / f64::from(a.contacts));
    row.contact_pending = a.pending;
}

/// 人ごとの行とチーム全体の行に、接触・日程切りの数を入れる。`agg` が無い (読めなかった) ときは何も入れない。
/// チーム全体は表に出ている人の合計。率の分母は各行の発信数からアポ取り以外の本数を引いたもの。
pub fn apply_contact(
    people: &mut [TalkPersonRow],
    total: &mut TalkPersonRow,
    agg: Option<&HashMap<String, ContactAgg>>,
) {
    let Some(agg) = agg else { return };
    let mut sum = ContactAgg::default();
    // 全体の分母は各行の分母の合計 (発信数の無い人の対象外の本数を全体から引かない)
    let mut den_sum: u32 = 0;
    for p in people.iter_mut() {
        let a = p
            .owner_id
            .as_ref()
            .and_then(|id| agg.get(id))
            .copied()
            .unwrap_or_default();
        sum.add(&a);
        let den = rate_den(p, &a);
        den_sum += den.unwrap_or(0);
        fill(p, &a, den);
    }
    let den = total.outbound.map(|_| den_sum);
    fill(total, &sum, den);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::summary::{build, Member, ALERT_RULES};
    use serde_json::json;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
    }
    fn maps() -> (
        HashMap<String, String>,
        HashMap<String, String>,
        HashSet<String>,
    ) {
        let users = HashMap::from([
            ("u1".to_string(), "a@f-a-c.co.jp".to_string()),
            ("u2".to_string(), "b@f-a-c.co.jp".to_string()),
            ("u3".to_string(), "outsider@f-a-c.co.jp".to_string()),
        ]);
        let owners = HashMap::from([
            ("a@f-a-c.co.jp".to_string(), "11".to_string()),
            ("b@f-a-c.co.jp".to_string(), "22".to_string()),
            ("outsider@f-a-c.co.jp".to_string(), "33".to_string()),
        ]);
        let allowed = HashSet::from(["11".to_string(), "22".to_string()]);
        (users, owners, allowed)
    }
    fn log(id: &str, dir: &str, at: &str, user: &str, result: &str) -> Value {
        json!({"id": id, "direction": dir, "date_time": at, "user_id": user, "result": result})
    }

    #[test]
    fn discovery_keeps_only_connected_outbound_of_target_people_in_the_jst_day() {
        let (users, owners, allowed) = maps();
        let pages = vec![
            json!({"call_logs": [
                log("1", "outbound", "2026-10-20T00:30:00Z", "u1", "Auto Recorded"),
                // 同じ id は 1 件
                log("1", "outbound", "2026-10-20T00:30:00Z", "u1", "Auto Recorded"),
                // 着信
                log("2", "inbound", "2026-10-20T01:00:00Z", "u1", "Auto Recorded"),
                // つながっていない発信は判定しない
                log("3", "outbound", "2026-10-20T01:00:00Z", "u1", "Call Cancel"),
                // 前日 (JST 0:00 の 1 秒前) / 翌日 (JST 24:00)
                log("4", "outbound", "2026-10-19T14:59:59Z", "u2", "Auto Recorded"),
                log("5", "outbound", "2026-10-20T15:00:00Z", "u2", "Auto Recorded"),
                // JST 23:59:59 は今日
                log("6", "outbound", "2026-10-20T14:59:59Z", "u2", "Auto Recorded"),
            ]}),
            json!({"call_logs": [
                {"call_id": "7", "direction": "outbound", "start_time": "2026-10-20T02:00:00Z", "user_id": "u1", "result": "Auto Recorded"},
                // user_id なし / 対象チームではない人 / 結び付かない Zoom ユーザー
                {"id": "8", "direction": "outbound", "date_time": "2026-10-20T02:00:00Z", "result": "Auto Recorded"},
                log("9", "outbound", "2026-10-20T02:00:00Z", "u3", "Auto Recorded"),
                log("10", "outbound", "2026-10-20T02:00:00Z", "u9", "Auto Recorded"),
            ]}),
        ];
        let c = discover(&pages, day(), &users, &owners, &allowed);
        let got: Vec<(&str, &str, &str)> = c
            .iter()
            .map(|c| (c.id.as_str(), c.owner_id.as_str(), c.call_at.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("1", "11", "2026-10-20T00:30:00Z"),
                ("6", "22", "2026-10-20T14:59:59Z"),
                ("7", "11", "2026-10-20T02:00:00Z"),
            ]
        );
    }

    #[test]
    fn parses_judge_output_including_malformed() {
        let ok = parse_text(
            "<think>考え</think>```json\n{\"contacted\": true, \"date_proposals\": 2}\n```",
        );
        assert_eq!(
            ok,
            Some(ContactResult {
                contacted: true,
                date_proposals: 2
            })
        );
        // 接触していなければ回数は 0
        assert_eq!(
            parse_text("{\"contacted\": false, \"date_proposals\": 3}"),
            Some(ContactResult {
                contacted: false,
                date_proposals: 0
            })
        );
        // 文字列の値・回数の欠け・上限
        assert_eq!(
            parse_text("{\"contacted\": \"true\", \"date_proposals\": \"1\"}"),
            Some(ContactResult {
                contacted: true,
                date_proposals: 1
            })
        );
        assert_eq!(
            parse_text("{\"contacted\": true}"),
            Some(ContactResult {
                contacted: true,
                date_proposals: 0
            })
        );
        assert_eq!(
            parse_text("{\"contacted\": true, \"date_proposals\": 999}")
                .unwrap()
                .date_proposals,
            MAX_PROPOSALS
        );
        // 壊れた応答
        assert_eq!(parse_text("JSON なし"), None);
        assert_eq!(parse_text("{\"date_proposals\": 2}"), None);
        assert_eq!(parse_text("{\"contacted\": \"たぶん\"}"), None);
    }

    fn turn(n: u32, who: &str, text: &str) -> TalkTurn {
        TalkTurn {
            n,
            speaker: who.into(),
            text: text.into(),
        }
    }

    #[test]
    fn prompt_has_definitions_and_is_cut_to_the_limit() {
        let p = build_prompt(&[turn(1, "rep", "もしもし"), turn(2, "customer", "はい")]);
        assert!(p.contains("1. 営: もしもし\n2. 先: はい\n"));
        assert!(p.contains("受付だけで終わった・留守電"));
        assert!(p.contains("{\"contacted\": true/false, \"date_proposals\": 回数}"));
        let long: Vec<TalkTurn> = (1..=2000)
            .map(|n| turn(n, "rep", "あいうえおかきくけこ"))
            .collect();
        let p = build_prompt(&long);
        assert!(p.chars().count() < MAX_PROMPT_CHARS + 600);
        assert!(!p.contains("2000. 営"));
    }

    struct FakeZoom(Result<Vec<TalkTurn>, ZoomError>);
    impl TranscriptSource for FakeZoom {
        async fn fetch(&self, _: &str) -> Result<Vec<TalkTurn>, ZoomError> {
            self.0.clone()
        }
        async fn find_recording(&self, _: &str, _: i64) -> Result<Option<String>, ZoomError> {
            Ok(None)
        }
    }
    struct FakeJudge(Result<ContactResult, JudgeError>);
    impl ContactJudge for FakeJudge {
        async fn judge(&self, _: &[TalkTurn]) -> Result<ContactResult, JudgeError> {
            self.0.clone()
        }
    }
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-20T06:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn both() -> Vec<TalkTurn> {
        vec![
            turn(1, "rep", "お世話になります"),
            turn(2, "customer", "はい"),
        ]
    }
    const FRESH: &str = "2026-10-20T05:00:00Z";

    #[tokio::test]
    async fn process_one_outcomes() {
        let ok = ContactResult {
            contacted: true,
            date_proposals: 2,
        };
        let j = FakeJudge(Ok(ok));
        assert_eq!(
            process_one(&FakeZoom(Ok(both())), &j, "i", FRESH, now()).await,
            Outcome::Judged(ok)
        );
        // 営業しか話していない通話 (留守電) は接触ではない。判定を呼ばずに確定
        let none = ContactResult {
            contacted: false,
            date_proposals: 0,
        };
        let rep_only = vec![turn(1, "rep", "留守電に失礼します")];
        assert_eq!(
            process_one(&FakeZoom(Ok(rep_only)), &j, "i", FRESH, now()).await,
            Outcome::Judged(none)
        );
        let cust_only = vec![turn(1, "customer", "はい")];
        assert_eq!(
            process_one(&FakeZoom(Ok(cust_only)), &j, "i", FRESH, now()).await,
            Outcome::Unavailable("speakers_not_separated")
        );
        assert_eq!(
            process_one(&FakeZoom(Ok(vec![])), &j, "i", FRESH, now()).await,
            Outcome::Retry("transcript_empty")
        );
        assert_eq!(
            process_one(&FakeZoom(Err(ZoomError::NotReady)), &j, "i", FRESH, now()).await,
            Outcome::Retry("transcript_not_ready")
        );
        // 短すぎる通話・文字起こしが無効 (Zoom の code 12000 / 12001): 待たずに取れない扱い
        assert_eq!(
            process_one(
                &FakeZoom(Err(ZoomError::Untranscribable)),
                &j,
                "i",
                FRESH,
                now()
            )
            .await,
            Outcome::Unavailable("untranscribable")
        );
        assert_eq!(
            decide(Outcome::Unavailable("untranscribable"), 0, now()),
            Update::Final {
                status: ST_UNAVAILABLE,
                attempts: 1
            }
        );
        // 録音が無い: 新しければ待つ、1 日以上前なら取れない
        assert_eq!(
            process_one(&FakeZoom(Err(ZoomError::Gone)), &j, "i", FRESH, now()).await,
            Outcome::Retry("recording_not_found_yet")
        );
        assert_eq!(
            process_one(
                &FakeZoom(Err(ZoomError::Gone)),
                &j,
                "i",
                "2026-10-19T05:00:00Z",
                now()
            )
            .await,
            Outcome::Unavailable("recording_gone")
        );
        // 判定の失敗: 混雑 (429) は別の印
        let busy = FakeJudge(Err(JudgeError::Transient("http 429".into())));
        assert_eq!(
            process_one(&FakeZoom(Ok(both())), &busy, "i", FRESH, now()).await,
            Outcome::Retry("judge_rate_limited")
        );
        let bad = FakeJudge(Err(JudgeError::Transient("json".into())));
        assert_eq!(
            process_one(&FakeZoom(Ok(both())), &bad, "i", FRESH, now()).await,
            Outcome::Retry("judge_error")
        );
    }

    #[test]
    fn decide_backs_off_and_gives_up_after_the_limit() {
        let n = now();
        assert_eq!(
            decide(Outcome::Retry("x"), 0, n),
            Update::Retry {
                attempts: 1,
                next: n + ChronoDuration::minutes(10)
            }
        );
        assert_eq!(
            decide(Outcome::Retry("x"), 2, n),
            Update::Retry {
                attempts: 3,
                next: n + ChronoDuration::minutes(60)
            }
        );
        assert_eq!(
            decide(Outcome::Retry("x"), MAX_ATTEMPTS - 1, n),
            Update::Final {
                status: "unavailable",
                attempts: MAX_ATTEMPTS
            }
        );
        assert_eq!(
            decide(Outcome::Unavailable("gone"), 0, n),
            Update::Final {
                status: "unavailable",
                attempts: 1
            }
        );
    }

    fn member(id: &str, name: &str) -> Member {
        Member {
            owner_id: id.into(),
            name: name.into(),
            team: Some("新規営業".into()),
        }
    }

    #[test]
    fn aggregates_per_person_and_total_with_outbound_as_the_denominator() {
        // A: 発信 100 本、接触 20、日程切り 8 本・合計 12 回、判定待ち 3
        // B: 発信 50 本、接触 0 (率は 0%、1 接触あたりは null)
        // C: 発信 0 本 (率は null)
        let members = vec![member("A", "あ"), member("B", "い"), member("C", "う")];
        let dials: HashMap<String, u32> =
            [("A".into(), 40), ("B".into(), 10), ("C".into(), 1)].into();
        let out: HashMap<String, u32> =
            [("A".into(), 100), ("B".into(), 50), ("C".into(), 0)].into();
        let (mut people, mut total) = build(
            &members,
            Some(&dials),
            Some(&out),
            &[],
            &[],
            &[],
            &ALERT_RULES,
        );
        let agg: HashMap<String, ContactAgg> = [
            (
                "A".to_string(),
                ContactAgg {
                    contacts: 20,
                    proposal_calls: 8,
                    proposals: 12,
                    pending: 3,
                    out_of_scope: 20,
                },
            ),
            // C は発信数 0 なのに対象外が 10 本ある (営業KPI に載っていない日など)。全体の分母から引かない
            (
                "C".to_string(),
                ContactAgg {
                    out_of_scope: 10,
                    ..Default::default()
                },
            ),
            (
                "Z".to_string(),
                ContactAgg {
                    contacts: 99,
                    proposal_calls: 99,
                    proposals: 99,
                    pending: 99,
                    out_of_scope: 99,
                },
            ),
        ]
        .into();
        apply_contact(&mut people, &mut total, Some(&agg));
        let by = |n: &str| people.iter().find(|p| p.name == n).unwrap();
        let a = by("あ");
        assert_eq!(a.contacts, Some(20));
        // 発信 100 本のうちアポ取り以外 20 本を分母から外す
        assert_eq!(a.contact_rate_pct, Some(25.0));
        assert_eq!(a.date_proposal_calls, Some(8));
        assert_eq!(a.date_proposal_rate_pct, Some(10.0));
        assert_eq!(a.proposals_per_contact, Some(0.6));
        assert_eq!(a.contact_pending, 3);
        let b = by("い");
        assert_eq!((b.contacts, b.contact_rate_pct), (Some(0), Some(0.0)));
        assert_eq!(
            (b.date_proposal_calls, b.date_proposal_rate_pct),
            (Some(0), Some(0.0))
        );
        assert_eq!(b.proposals_per_contact, None);
        let c = by("う");
        assert_eq!(c.contacts, Some(0));
        assert_eq!((c.contact_rate_pct, c.date_proposal_rate_pct), (None, None));
        // 合計: 表に出ている人だけ (Z は入らない)。分母は各行の分母の合計 = 80 + 50 + 0 (C の 10 本は引かない)
        assert_eq!(total.contacts, Some(20));
        assert_eq!(
            total.contact_rate_pct.map(|v| (v * 100.0).round() / 100.0),
            Some(15.38)
        );
        assert_eq!(total.date_proposal_calls, Some(8));
        assert_eq!(total.proposals_per_contact, Some(0.6));
        assert_eq!(total.contact_pending, 3);
    }

    #[test]
    fn rates_are_null_without_outbound_and_nothing_is_filled_without_marks() {
        let members = vec![member("A", "あ")];
        let dials: HashMap<String, u32> = [("A".into(), 5)].into();
        let (mut people, mut total) =
            build(&members, Some(&dials), None, &[], &[], &[], &ALERT_RULES);
        let agg: HashMap<String, ContactAgg> = [(
            "A".to_string(),
            ContactAgg {
                contacts: 4,
                proposal_calls: 2,
                proposals: 3,
                pending: 0,
                out_of_scope: 0,
            },
        )]
        .into();
        apply_contact(&mut people, &mut total, Some(&agg));
        assert_eq!(people[0].contacts, Some(4));
        assert_eq!(
            people[0].contact_rate_pct, None,
            "発信数が分からなければ率は出さない"
        );
        assert_eq!(people[0].proposals_per_contact, Some(0.75));
        // 判定の保存が読めなかったとき
        let (mut p2, mut t2) = build(&members, Some(&dials), None, &[], &[], &[], &ALERT_RULES);
        apply_contact(&mut p2, &mut t2, None);
        assert_eq!((p2[0].contacts, t2.contacts), (None, None));
    }
}
