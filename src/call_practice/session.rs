//! 練習の状態。プロセス内メモリだけに持つ (保存しない)。単一インスタンス前提 (record_lock などと同じ)。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::persona::{is_date_offer, Persona, Range, NEVER_END_OFFERS};
use super::roleplay::{self, ChatMsg, CustomerReply, MemoCtx, OfferInfo};
use super::score::RepMark;
use super::types::*;
use crate::crm::talk_review::types::TalkTurn;

/// 営業の発言の上限
pub const MAX_TURNS: u32 = 30;
/// 1 回の練習の長さの上限 (分)
pub const MAX_MINUTES: u32 = 15;
/// 最後の操作からこの時間で消す
const IDLE_TTL: Duration = Duration::from_secs(2 * 60 * 60);
/// 全体で持つ練習の数
const MAX_SESSIONS: usize = 500;
pub const MAX_TEXT_CHARS: usize = 300;

pub struct Session {
    pub owner: String,
    pub persona: &'static Persona,
    pub range: Range,
    /// 日時が決まったか (ヒアリングだけの範囲は最初から true)
    pub agreed: bool,
    pub agreed_turn: Option<u32>,
    pub agreed_secs: Option<u32>,
    pub offer_turns: Vec<u32>,
    /// 日時の打診を断ったお客さん役の発言の番号
    pub objection_turns: Vec<u32>,
    pub transcript: Vec<TalkTurn>,
    pub marks: Vec<RepMark>,
    pub irritated_turns: Vec<u32>,
    pub question_count: u32,
    pub fee_told: bool,
    pub budget_at_ask: Option<String>,
    pub history: Vec<ChatMsg>,
    pub last_turn: Option<(String, PracticeTurnResponse)>,
    pub busy: bool,
    pub ended: bool,
    pub result: Option<PracticeResult>,
    pub script_seen: bool,
    pub started: Instant,
    pub touched: Instant,
}

/// 発言を受け付けたとき、返事を作るのに要るもの
pub struct TurnPlan {
    /// 今回の発言が日時の打診なら、その扱い
    pub offer: Option<OfferInfo>,
    pub system: String,
    pub messages: Vec<ChatMsg>,
    pub rep_text: String,
    pub client_turn_id: String,
}

pub enum Begin {
    NotFound,
    Finished,
    Closed,
    InProgress,
    Replay(PracticeTurnResponse),
    Ready(TurnPlan),
}

#[derive(Default)]
pub struct Store {
    inner: Mutex<HashMap<String, Session>>,
}

fn is_fee_text(t: &str) -> bool {
    t.contains("月額") || t.contains("定額")
}

fn is_price_question(t: &str) -> bool {
    t.contains("いくら")
        || t.contains("何円")
        || t.contains("金額を教")
        || t.contains("金額はいくら")
}

impl Store {
    pub fn create(
        &self,
        owner: &str,
        persona: &'static Persona,
        range: Range,
        script_seen: bool,
    ) -> Result<(String, TalkTurn), &'static str> {
        let now = Instant::now();
        let mut m = self.inner.lock().expect("session store");
        m.retain(|_, s| now.duration_since(s.touched) < IDLE_TTL);
        if m.len() >= MAX_SESSIONS {
            return Err("sessions_full");
        }
        let id = uuid::Uuid::new_v4().to_string();
        let opening = TalkTurn {
            n: 1,
            speaker: "customer".into(),
            text: persona.first_line(range).to_string(),
        };
        m.insert(
            id.clone(),
            Session {
                owner: owner.to_string(),
                persona,
                range,
                agreed: !range.has_appointment(),
                agreed_turn: None,
                agreed_secs: None,
                offer_turns: vec![],
                objection_turns: vec![],
                transcript: vec![opening.clone()],
                marks: vec![],
                irritated_turns: vec![],
                question_count: 0,
                fee_told: false,
                budget_at_ask: None,
                history: vec![],
                last_turn: None,
                busy: false,
                ended: false,
                result: None,
                script_seen,
                started: now,
                touched: now,
            },
        );
        Ok((id, opening))
    }

    pub fn view(&self, id: &str, owner: &str) -> Option<PracticeSessionView> {
        let mut m = self.inner.lock().expect("session store");
        let s = m.get_mut(id).filter(|s| s.owner == owner)?;
        s.touched = Instant::now();
        Some(PracticeSessionView {
            session_id: id.to_string(),
            range: s.range.as_str().to_string(),
            date_agreed: s.agreed,
            transcript: s.transcript.clone(),
            ended: s.ended || s.result.is_some(),
            finished: s.result.is_some(),
            max_turns: MAX_TURNS,
            script_seen: s.script_seen,
            result: s.result.clone(),
        })
    }

    pub fn begin_turn(&self, id: &str, owner: &str, client_turn_id: &str, text: &str) -> Begin {
        let mut m = self.inner.lock().expect("session store");
        let Some(s) = m.get_mut(id).filter(|s| s.owner == owner) else {
            return Begin::NotFound;
        };
        let now = Instant::now();
        s.touched = now;
        if s.result.is_some() {
            return Begin::Finished;
        }
        if let Some((cid, resp)) = &s.last_turn {
            if cid == client_turn_id {
                return Begin::Replay(resp.clone());
            }
        }
        if s.busy {
            return Begin::InProgress;
        }
        if s.ended
            || now.duration_since(s.started) > Duration::from_secs(u64::from(MAX_MINUTES) * 60)
        {
            s.ended = true;
            return Begin::Closed;
        }
        s.busy = true;
        // これまでの会話 + 今回の営業の発言 (進行メモ付き)
        let offer = if !s.agreed && is_date_offer(text) {
            let n = s.offer_turns.len() as u32 + 1;
            Some(OfferInfo {
                n,
                accept: s.persona.accepts_offer(n),
                give_up: s.persona.appointment.accept_after.is_none() && n >= NEVER_END_OFFERS,
            })
        } else {
            None
        };
        let memo = roleplay::progress_memo(
            &MemoCtx {
                question_count: s.question_count,
                fee_told: s.fee_told || is_fee_text(text),
                agreed: s.agreed,
                offer,
            },
            s.persona,
        );
        let mut messages = s.history.clone();
        messages.push(ChatMsg {
            role: "user",
            content: format!("{text}{memo}"),
        });
        Begin::Ready(TurnPlan {
            offer,
            system: roleplay::build_system(s.persona, s.range),
            messages,
            rep_text: text.to_string(),
            client_turn_id: client_turn_id.to_string(),
        })
    }

    /// 返事が取れなかったとき、発言を取り消して次を受けられるようにする
    pub fn abort_turn(&self, id: &str) {
        if let Some(s) = self.inner.lock().expect("session store").get_mut(id) {
            s.busy = false;
        }
    }

    pub fn complete_turn(
        &self,
        id: &str,
        plan: &TurnPlan,
        reply: CustomerReply,
    ) -> Option<PracticeTurnResponse> {
        let mut m = self.inner.lock().expect("session store");
        let s = m.get_mut(id)?;
        s.busy = false;
        s.touched = Instant::now();
        let rep_n = s.transcript.len() as u32 + 1;
        let cust_n = rep_n + 1;
        if is_fee_text(&plan.rep_text) {
            s.fee_told = true;
        }
        let slots = reply.asked_slots.clone();
        // 4 項目の質問の数は、日時が決まった後だけ数える
        let agreed_before = s.agreed;
        if reply.is_question && agreed_before {
            s.question_count += 1;
        }
        let mut force_end = false;
        if !agreed_before {
            if let Some(o) = plan.offer {
                s.offer_turns.push(rep_n);
                if o.accept {
                    s.agreed = true;
                    s.agreed_turn = Some(cust_n);
                    s.agreed_secs = Some(s.started.elapsed().as_secs() as u32);
                } else {
                    s.objection_turns.push(cust_n);
                    force_end = o.give_up;
                }
            } else if reply.objection {
                s.objection_turns.push(cust_n);
            }
        }
        if slots.iter().any(|x| x == "budget") && s.budget_at_ask.is_none() {
            s.budget_at_ask = Some(s.persona.budget_final(s.fee_told));
        }
        s.marks.push(RepMark {
            turn: rep_n,
            slots,
            is_question: reply.is_question,
            asked_price: is_price_question(&plan.rep_text),
        });
        let rep_turn = TalkTurn {
            n: rep_n,
            speaker: "rep".into(),
            text: plan.rep_text.clone(),
        };
        let customer_turn = TalkTurn {
            n: cust_n,
            speaker: "customer".into(),
            text: reply.text.clone(),
        };
        if reply.irritated && agreed_before {
            s.irritated_turns.push(cust_n);
        }
        s.transcript.push(rep_turn.clone());
        s.transcript.push(customer_turn.clone());
        // 次の呼び出しには進行メモ無しの本文を渡す (メモは毎回最新のものだけを付ける)
        s.history.push(ChatMsg {
            role: "user",
            content: plan.rep_text.clone(),
        });
        s.history.push(ChatMsg {
            role: "assistant",
            content: reply.text.clone(),
        });
        let rep_count = s.marks.len() as u32;
        let over = s.question_count > s.persona.personality.patience + 3;
        if reply.end
            || rep_count >= MAX_TURNS
            || over
            || force_end
            || (s.agreed && s.range == Range::Appointment)
        {
            s.ended = true;
        }
        let resp = PracticeTurnResponse {
            rep_turn,
            customer_turn,
            ended: s.ended,
            date_agreed: s.agreed,
        };
        s.last_turn = Some((plan.client_turn_id.clone(), resp.clone()));
        Some(resp)
    }

    /// 採点に要る材料を取り出す。終えた練習なら保存済みの結果を返す
    pub fn snapshot_for_finish(&self, id: &str, owner: &str) -> Result<Snapshot, FinishErr> {
        let mut m = self.inner.lock().expect("session store");
        let s = m
            .get_mut(id)
            .filter(|s| s.owner == owner)
            .ok_or(FinishErr::NotFound)?;
        s.touched = Instant::now();
        if let Some(r) = &s.result {
            return Err(FinishErr::Done(Box::new(r.clone())));
        }
        if s.busy {
            return Err(FinishErr::InProgress);
        }
        s.busy = true;
        Ok(Snapshot {
            persona: s.persona,
            range: s.range,
            agreed: s.agreed,
            agreed_turn: s.agreed_turn,
            agreed_secs: s.agreed_secs,
            offer_turns: s.offer_turns.clone(),
            objection_turns: s.objection_turns.clone(),
            transcript: s.transcript.clone(),
            marks: s.marks.clone(),
            irritated_turns: s.irritated_turns.clone(),
            question_count: s.question_count,
            fee_told: s.fee_told,
            budget_at_ask: s.budget_at_ask.clone(),
            script_seen: s.script_seen,
        })
    }

    pub fn store_result(&self, id: &str, result: PracticeResult) {
        if let Some(s) = self.inner.lock().expect("session store").get_mut(id) {
            s.busy = false;
            s.ended = true;
            s.result = Some(result);
        }
    }

    pub fn release(&self, id: &str) {
        self.abort_turn(id);
    }

    #[cfg(test)]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.inner.lock().expect("session store").len()
    }
}

impl Begin {
    #[cfg(test)]
    pub fn is_ready(&self) -> bool {
        matches!(self, Begin::Ready(_))
    }
}

pub struct Snapshot {
    pub persona: &'static Persona,
    pub range: Range,
    pub agreed: bool,
    pub agreed_turn: Option<u32>,
    pub agreed_secs: Option<u32>,
    pub offer_turns: Vec<u32>,
    pub objection_turns: Vec<u32>,
    pub transcript: Vec<TalkTurn>,
    pub marks: Vec<RepMark>,
    pub irritated_turns: Vec<u32>,
    pub question_count: u32,
    pub fee_told: bool,
    pub budget_at_ask: Option<String>,
    pub script_seen: bool,
}

pub enum FinishErr {
    NotFound,
    InProgress,
    Done(Box<PracticeResult>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_practice::persona;

    fn reply(text: &str, slots: &[&str], q: bool, end: bool) -> CustomerReply {
        CustomerReply {
            text: text.into(),
            asked_slots: slots.iter().map(|s| s.to_string()).collect(),
            is_question: q,
            irritated: false,
            objection: false,
            end,
        }
    }

    #[test]
    fn 発言の番号は通し番号で最初の一言が1番() {
        let st = Store::default();
        let p = persona::find("hot_01").unwrap();
        let (id, op) = st
            .create("a@f-a-c.co.jp", p, Range::Hearing, false)
            .unwrap();
        assert_eq!((op.n, op.speaker.as_str()), (1, "customer"));
        let Begin::Ready(plan) = st.begin_turn(&id, "a@f-a-c.co.jp", "t1", "職種はありますか")
        else {
            panic!()
        };
        let r = st
            .complete_turn(
                &id,
                &plan,
                reply("ドライバーを2名です", &["hiring_3m"], true, false),
            )
            .unwrap();
        assert_eq!((r.rep_turn.n, r.customer_turn.n), (2, 3));
        assert!(!r.ended);
        let v = st.view(&id, "a@f-a-c.co.jp").unwrap();
        assert_eq!(v.transcript.len(), 3);
    }

    #[test]
    fn 同じ番号の再送は前の返事を返し二重に進まない() {
        let st = Store::default();
        let p = persona::find("hot_01").unwrap();
        let (id, _) = st.create("a", p, Range::Hearing, false).unwrap();
        let Begin::Ready(plan) = st.begin_turn(&id, "a", "t1", "こんにちは") else {
            panic!()
        };
        let r1 = st
            .complete_turn(&id, &plan, reply("はい", &[], false, false))
            .unwrap();
        let Begin::Replay(r2) = st.begin_turn(&id, "a", "t1", "こんにちは") else {
            panic!("再送は Replay")
        };
        assert_eq!(r1, r2);
        assert_eq!(st.view(&id, "a").unwrap().transcript.len(), 3);
    }

    #[test]
    fn 返事の途中で次を受けず他人の練習は見えない() {
        let st = Store::default();
        let p = persona::find("hot_01").unwrap();
        let (id, _) = st.create("a", p, Range::Hearing, false).unwrap();
        assert!(matches!(
            st.begin_turn(&id, "a", "t1", "x"),
            Begin::Ready(_)
        ));
        assert!(matches!(
            st.begin_turn(&id, "a", "t2", "y"),
            Begin::InProgress
        ));
        st.abort_turn(&id);
        assert!(matches!(
            st.begin_turn(&id, "a", "t2", "y"),
            Begin::Ready(_)
        ));
        assert!(matches!(
            st.begin_turn(&id, "b", "t3", "z"),
            Begin::NotFound
        ));
        assert!(st.view(&id, "b").is_none());
    }

    #[test]
    fn 質問の数と月額定額と予算の時点を数える() {
        let st = Store::default();
        let p = persona::find("budget_01").unwrap();
        let (id, _) = st.create("a", p, Range::Hearing, false).unwrap();
        let Begin::Ready(p1) = st.begin_turn(&id, "a", "1", "予算はいくらですか") else {
            panic!()
        };
        st.complete_turn(&id, &p1, reply("特にないです", &["budget"], true, false));
        let Begin::Ready(p2) =
            st.begin_turn(&id, "a", "2", "弊社は月額定額です。ご検討いただけますか")
        else {
            panic!()
        };
        // 進行メモには「伝えられた」が入る
        assert!(p2.messages.last().unwrap().content.contains("伝えられた"));
        st.complete_turn(&id, &p2, reply("内容次第です", &["consider"], true, false));
        let snap = match st.snapshot_for_finish(&id, "a") {
            Ok(s) => s,
            Err(_) => panic!(),
        };
        assert_eq!(snap.question_count, 2);
        assert!(snap.fee_told);
        assert_eq!(snap.budget_at_ask.as_deref(), Some("no"));
        assert!(snap.marks[0].asked_price);
        assert!(!snap.marks[1].asked_price);
    }

    #[test]
    fn 往復の上限か会話の終了で閉じて閉じた後は受けない() {
        let st = Store::default();
        let p = persona::find("hot_01").unwrap();
        let (id, _) = st.create("a", p, Range::Hearing, false).unwrap();
        let Begin::Ready(plan) = st.begin_turn(&id, "a", "1", "失礼いたします") else {
            panic!()
        };
        let r = st
            .complete_turn(&id, &plan, reply("失礼します", &[], false, true))
            .unwrap();
        assert!(r.ended);
        assert!(matches!(
            st.begin_turn(&id, "a", "2", "もう一つ"),
            Begin::Closed
        ));
        // 上限の 30 回
        let (id2, _) = st.create("a", p, Range::Hearing, false).unwrap();
        let mut last = false;
        for i in 0..MAX_TURNS {
            let Begin::Ready(pl) = st.begin_turn(&id2, "a", &format!("t{i}"), "はい") else {
                panic!()
            };
            last = st
                .complete_turn(&id2, &pl, reply("ええ", &[], false, false))
                .unwrap()
                .ended;
        }
        assert!(last);
        assert_eq!(st.len(), 2);
    }

    fn appt_turn(st: &Store, id: &str, cid: &str, text: &str) -> TurnPlan {
        match st.begin_turn(id, "a", cid, text) {
            Begin::Ready(p) => p,
            _ => panic!("ready ではない"),
        }
    }

    #[test]
    fn 日時を決める範囲は担当者の最初の一言から始まり_受ける回数まで断って受けたら終わる() {
        let st = Store::default();
        let p = persona::find("budget_01").unwrap();
        let (id, op) = st.create("a", p, Range::Appointment, false).unwrap();
        assert_eq!(op.text, "はい、お電話代わりました");
        let v = st.view(&id, "a").unwrap();
        assert_eq!((v.range.as_str(), v.date_agreed), ("appointment", false));
        // 用件だけ: 打診ではない
        let p1 = appt_turn(&st, &id, "1", "採用のご支援の件でお電話しました");
        assert!(p1.offer.is_none());
        assert!(p1
            .messages
            .last()
            .unwrap()
            .content
            .contains("日時の打診ではない"));
        st.complete_turn(&id, &p1, reply("はあ", &[], false, false));
        // 1 回目の打診: 断る
        let p2 = appt_turn(&st, &id, "2", "来週の水曜か木曜あたりはいかがですか");
        let o = p2.offer.unwrap();
        assert_eq!((o.n, o.accept, o.give_up), (1, false, false));
        assert!(p2
            .messages
            .last()
            .unwrap()
            .content
            .contains("今は大丈夫ですね"));
        let r2 = st
            .complete_turn(&id, &p2, reply("今は大丈夫ですね", &[], false, false))
            .unwrap();
        assert!(!r2.ended && !r2.date_agreed);
        // 2 回目の打診: 受ける (この型は 2 回目)
        let p3 = appt_turn(&st, &id, "3", "では再来週の火曜はいかがですか");
        assert!(p3.offer.unwrap().accept);
        let r3 = st
            .complete_turn(&id, &p3, reply("じゃあ、その日で", &[], false, false))
            .unwrap();
        assert!(r3.date_agreed && r3.ended);
        let snap = match st.snapshot_for_finish(&id, "a") {
            Ok(s) => s,
            Err(_) => panic!(),
        };
        assert_eq!(snap.offer_turns, [4, 6]);
        assert_eq!(snap.objection_turns, [5]);
        assert_eq!(snap.agreed_turn, Some(7));
        assert!(snap.agreed);
        // 日時が決まる前の質問は 4 項目の質問の数に入らない
        assert_eq!(snap.question_count, 0);
    }

    #[test]
    fn 受けない型は4回目の打診で電話を切る() {
        let st = Store::default();
        let p = persona::find("cold_01").unwrap();
        let (id, _) = st.create("a", p, Range::Full, false).unwrap();
        for i in 1..=4u32 {
            let pl = appt_turn(&st, &id, &format!("t{i}"), "来週の水曜はいかがですか");
            let o = pl.offer.unwrap();
            assert_eq!((o.n, o.accept, o.give_up), (i, false, i == 4));
            let r = st
                .complete_turn(&id, &pl, reply("結構です", &[], false, false))
                .unwrap();
            assert_eq!(r.ended, i == 4);
            assert!(!r.date_agreed);
        }
    }

    #[test]
    fn 通しは日時が決まった後も続き_決まった後の質問から数える() {
        let st = Store::default();
        let p = persona::find("hot_01").unwrap();
        let (id, _) = st.create("a", p, Range::Full, false).unwrap();
        let pl = appt_turn(&st, &id, "1", "来週の水曜か木曜あたりはいかがですか");
        assert!(pl.offer.unwrap().accept);
        let r = st
            .complete_turn(&id, &pl, reply("ええ、その日で", &[], false, false))
            .unwrap();
        assert!(r.date_agreed && !r.ended);
        let pl2 = appt_turn(
            &st,
            &id,
            "2",
            "直近3か月以内で、採用が必要な職種はありますか",
        );
        assert!(pl2.offer.is_none());
        assert!(pl2
            .messages
            .last()
            .unwrap()
            .content
            .contains("営業の質問はここまで0問"));
        st.complete_turn(
            &id,
            &pl2,
            reply("ドライバーです", &["hiring_3m"], true, false),
        );
        match st.snapshot_for_finish(&id, "a") {
            Ok(s) => assert_eq!(s.question_count, 1),
            Err(_) => panic!(),
        }
    }
}
