//! お客さん役の型 (`data/personas.json`)。架空の会社・人物だけ。中身は型ごとに固定でランダムに変えない。

use std::sync::OnceLock;

use serde::Deserialize;

/// 聞かれる項目 (台本 v1 の 4 項目を分けたもの)
pub const SLOTS: [&str; 8] = [
    "hiring_3m",
    "hiring_later",
    "issue",
    "urgency",
    "consider",
    "budget",
    "decider",
    "attend",
];

#[derive(Debug, Clone, Deserialize)]
pub struct Setting {
    pub industry: String,
    pub size: String,
    pub speaker_title: String,
    /// 日時が決まった直後 (ヒアリングだけの範囲) の最初の一言
    pub opening: String,
    /// 担当者につながった直後 (日時を決める範囲・通し) の最初の一言
    pub person_opening: String,
}

/// 日時の打診への反応 (型ごとに固定)
#[derive(Debug, Clone, Deserialize)]
pub struct Appointment {
    /// 担当者の様子 (お客さん役に渡す)
    pub stance: String,
    /// 日時を受けるまでに出す断り文句。打診の 1 回目から順に使う
    pub objections: Vec<String>,
    /// 何回目の打診で日時を受けるか。無ければ最後まで受けない
    pub accept_after: Option<u32>,
    /// 受けるときの返事の例
    pub accept_say: String,
}

/// 練習の範囲
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    /// 担当者につながったところ → 日時が決まるまで
    Appointment,
    /// 日時が決まった後 → 4 項目を聞く
    Hearing,
    /// 担当者につながったところ → 日時 → 4 項目
    Full,
}

impl Range {
    pub fn parse(s: &str) -> Option<Range> {
        match s {
            "appointment" => Some(Range::Appointment),
            "hearing" => Some(Range::Hearing),
            "full" => Some(Range::Full),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Range::Appointment => "appointment",
            Range::Hearing => "hearing",
            Range::Full => "full",
        }
    }
    /// 日時を決める場面を含むか
    pub fn has_appointment(self) -> bool {
        self != Range::Hearing
    }
    /// 4 項目を聞く場面を含むか
    pub fn has_hearing(self) -> bool {
        self != Range::Appointment
    }
}

/// 受けない型が、これだけ打診されたら電話を切る
pub const NEVER_END_OFFERS: u32 = 4;

/// 営業の発言が、具体的な日時 (週・曜日・日・時間帯) を出した打診か
pub fn is_date_offer(t: &str) -> bool {
    const WORDS: [&str; 10] = [
        "来週",
        "再来週",
        "今週",
        "来月",
        "曜",
        "明日",
        "明後日",
        "午前",
        "午後",
        "あさって",
    ];
    if WORDS.iter().any(|w| t.contains(w)) {
        return true;
    }
    let chars: Vec<char> = t.chars().collect();
    chars.windows(2).any(|w| {
        (w[0].is_ascii_digit() || ('０'..='９').contains(&w[0])) && (w[1] == '日' || w[1] == '時')
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct Personality {
    pub tone: String,
    pub talkative: u8,
    pub guard: u8,
    /// 嫌がり始めるまでの質問の数
    pub patience: u32,
}

/// 職種・課題のように文で答える項目
#[derive(Debug, Clone, Deserialize)]
pub struct TextAnswer {
    pub say: String,
    pub truth: String,
    #[serde(default)]
    pub multi: bool,
    /// 記録が合っているかの条件。外側は全部、内側はどれか 1 つ含めばよい
    #[serde(rename = "match")]
    pub matches: Vec<Vec<String>>,
}

/// 選択肢で答える項目
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub value: String,
    pub say: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hidden {
    pub hiring_3m: TextAnswer,
    pub hiring_later: TextAnswer,
    pub issue: TextAnswer,
    pub urgency: Choice,
    pub consider: Choice,
    pub budget: Choice,
    pub decider: Choice,
    pub attend: Choice,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Persona {
    pub id: String,
    pub label: String,
    pub heat: String,
    pub reference: String,
    pub setting: Setting,
    pub personality: Personality,
    pub appointment: Appointment,
    pub hidden: Hidden,
    #[serde(default)]
    pub style_notes: Vec<String>,
    #[serde(default)]
    pub phrases: Vec<String>,
}

const RAW: &str = include_str!("data/personas.json");

#[derive(Debug, Clone, Deserialize)]
pub struct RealPhrase {
    pub text: String,
    pub calls: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RealPhrases {
    pub calls_scanned: u32,
    pub scenes: std::collections::BTreeMap<String, Vec<RealPhrase>>,
}

const REAL_RAW: &str = include_str!("data/real_phrases.json");

/// 実際の架電で担当者が返した短い返事 (場面ごと)
pub fn real_phrases() -> &'static RealPhrases {
    static R: OnceLock<RealPhrases> = OnceLock::new();
    R.get_or_init(|| serde_json::from_str(REAL_RAW).expect("real_phrases.json が読めない"))
}

pub fn all() -> &'static [Persona] {
    static P: OnceLock<Vec<Persona>> = OnceLock::new();
    P.get_or_init(|| serde_json::from_str(RAW).expect("personas.json が読めない"))
}

pub fn find(id: &str) -> Option<&'static Persona> {
    all().iter().find(|p| p.id == id)
}

/// サーバがランダムに 1 つ選ぶ (同じ型が続いてもよい)
pub fn pick_random() -> &'static Persona {
    use rand::Rng;
    let all = all();
    &all[rand::thread_rng().gen_range(0..all.len())]
}

pub fn urgency_label(v: &str) -> &'static str {
    match v {
        "early" => "早く解決したい",
        "not_so" => "そこまでではない",
        _ => "困っていない",
    }
}
pub fn consider_label(v: &str) -> &'static str {
    match v {
        "positive" => "前向き",
        "depends" => "内容と金額次第",
        "difficult" => "難しい",
        _ => "自分では決められない",
    }
}
pub fn budget_label(v: &str) -> &'static str {
    match v {
        "yes" => "ある",
        "no" => "無い",
        _ => "答えなかった",
    }
}
pub fn decider_label(v: &str) -> &'static str {
    match v {
        "self" => "本人",
        "self_plus_one" => "本人 (もう 1 人に一声かける)",
        "boss" => "上の人",
        _ => "本社・別部署",
    }
}
pub fn attend_label(v: &str) -> &'static str {
    match v {
        "yes" => "来る",
        "no" => "来ない",
        _ => "未定",
    }
}

impl Persona {
    pub fn text_answer(&self, slot: &str) -> Option<&TextAnswer> {
        match slot {
            "hiring_3m" => Some(&self.hidden.hiring_3m),
            "hiring_later" => Some(&self.hidden.hiring_later),
            "issue" => Some(&self.hidden.issue),
            _ => None,
        }
    }

    pub fn choice(&self, slot: &str) -> Option<&Choice> {
        match slot {
            "urgency" => Some(&self.hidden.urgency),
            "consider" => Some(&self.hidden.consider),
            "budget" => Some(&self.hidden.budget),
            "decider" => Some(&self.hidden.decider),
            "attend" => Some(&self.hidden.attend),
            _ => None,
        }
    }

    /// 予算の最終的な答え。「予算なし→変わる」型は、月額定額を聞いたあとなら ある、前なら 無い
    pub fn budget_final(&self, fee_told: bool) -> String {
        match self.hidden.budget.value.as_str() {
            "no_then_yes" => if fee_told { "yes" } else { "no" }.to_string(),
            v => v.to_string(),
        }
    }

    /// 台本の「熱い印」の数。隠れた答えから規則で決める (予算は最終的な答え)
    pub fn hot_marks(&self) -> Vec<&'static str> {
        let mut m = vec![];
        if self.hidden.hiring_3m.multi {
            m.push("hiring_3m");
        }
        if self.hidden.urgency.value == "early" {
            m.push("urgency");
        }
        if matches!(self.hidden.consider.value.as_str(), "positive" | "depends") {
            m.push("consider");
        }
        // 予算は、月額定額を聞けば ある になる型も含めて「ある」に変わり得るものを印とする
        if matches!(self.hidden.budget.value.as_str(), "yes" | "no_then_yes") {
            m.push("budget");
        }
        if self.hidden.attend.value == "yes" {
            m.push("attend");
        }
        m
    }

    /// 範囲に合わせた最初の一言
    pub fn first_line(&self, range: Range) -> &str {
        if range.has_appointment() {
            &self.setting.person_opening
        } else {
            &self.setting.opening
        }
    }

    /// 日時の打診 `n` 回目 (1 から) を受けるか
    pub fn accepts_offer(&self, n: u32) -> bool {
        self.appointment.accept_after.is_some_and(|k| n >= k)
    }

    pub fn objection_for(&self, n: u32) -> &str {
        let o = &self.appointment.objections;
        if o.is_empty() {
            return "";
        }
        &o[((n.max(1) - 1) as usize).min(o.len() - 1)]
    }

    pub fn setting_text(&self) -> String {
        format!(
            "{}の{} ({})",
            self.setting.industry, self.setting.speaker_title, self.setting.size
        )
    }
}

/// 熱い印の数から熱さを決める (0〜1 冷めている、2 どちらとも、3 以上 熱い)
pub fn heat_from_marks(n: u32) -> &'static str {
    match n {
        0 | 1 => "cold",
        2 => "mixed",
        _ => "hot",
    }
}

pub fn heat_label(h: &str) -> &'static str {
    match h {
        "hot" => "熱い",
        "mixed" => "どちらとも言えない",
        _ => "冷めている",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 型は8つで熱い印の数から決まる熱さが手で付けた熱さと一致する() {
        let ids: Vec<&str> = all().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "hot_01",
                "cold_01",
                "budget_01",
                "talkative_01",
                "busy_01",
                "docs_01",
                "later_01",
                "heard_01"
            ]
        );
        for p in all() {
            assert_eq!(
                heat_from_marks(p.hot_marks().len() as u32),
                p.heat,
                "{}",
                p.id
            );
        }
        assert_eq!(find("hot_01").unwrap().hot_marks().len(), 5);
        assert_eq!(find("cold_01").unwrap().hot_marks().len(), 0);
        assert_eq!(
            find("budget_01").unwrap().hot_marks(),
            ["consider", "budget"]
        );
    }

    #[test]
    fn 予算なし型は月額定額を聞いた後だけ予算ありになる() {
        let p = find("budget_01").unwrap();
        assert_eq!(p.budget_final(false), "no");
        assert_eq!(p.budget_final(true), "yes");
        assert_eq!(find("cold_01").unwrap().budget_final(true), "no");
    }

    #[test]
    fn 実在の連絡先らしい文字を含まない() {
        for raw in [RAW, REAL_RAW] {
            assert!(!raw.contains("株式会社") && !raw.contains("@") && !raw.contains("http"));
        }
        let raw = RAW;
        let digits_run = raw
            .chars()
            .collect::<Vec<_>>()
            .windows(6)
            .any(|w| w.iter().all(|c| c.is_ascii_digit()));
        assert!(!digits_run);
    }

    #[test]
    fn 日時の打診は具体的な週曜日日時を出した発言だけ数える() {
        assert!(is_date_offer("来週の水曜か木曜あたりはいかがですか"));
        assert!(is_date_offer("30日の午後はいかがですか"));
        assert!(is_date_offer("３日で"));
        assert!(!is_date_offer("お話しする機会をいただけませんか"));
        assert!(!is_date_offer("採用のご支援の件です"));
    }

    #[test]
    fn 型ごとの日時の受け方は固定で受けない型は最後まで受けない() {
        let a = |id: &str| find(id).unwrap();
        assert!(a("hot_01").accepts_offer(1));
        assert!(!a("budget_01").accepts_offer(1) && a("budget_01").accepts_offer(2));
        assert!(!a("docs_01").accepts_offer(2) && a("docs_01").accepts_offer(3));
        assert!(!a("heard_01").accepts_offer(2) && a("heard_01").accepts_offer(3));
        assert!(!a("cold_01").accepts_offer(1) && !a("cold_01").accepts_offer(99));
        assert_eq!(a("cold_01").objection_for(2), "うち、足りてるんで結構です");
        assert_eq!(
            a("busy_01").objection_for(1),
            "今ちょっと忙しくて、その日は難しいですね"
        );
        for p in all() {
            // 断り文句の数は、受けるまでの打診の数 - 1 以上ある
            if let Some(k) = p.appointment.accept_after {
                assert!(p.appointment.objections.len() as u32 >= k - 1, "{}", p.id);
                assert!(!p.appointment.accept_say.is_empty(), "{}", p.id);
            } else {
                assert!(
                    p.appointment.objections.len() as u32 >= NEVER_END_OFFERS,
                    "{}",
                    p.id
                );
            }
        }
    }

    #[test]
    fn 実際の返事の例は固有名詞らしい語を含まず2本以上の通話に出たものだけ() {
        let r = real_phrases();
        assert_eq!(r.calls_scanned, 2022);
        let mut n = 0;
        for (scene, list) in &r.scenes {
            assert!([
                "person_greet",
                "after_purpose",
                "after_date_offer",
                "refusal"
            ]
            .contains(&scene.as_str()));
            for ph in list {
                n += 1;
                assert!(ph.calls >= 2, "{}", ph.text);
                assert!(ph.text.chars().count() <= 45, "{}", ph.text);
                assert!(
                    !ph.text
                        .chars()
                        .any(|c| c.is_ascii_alphanumeric() || ('０'..='９').contains(&c)),
                    "{}",
                    ph.text
                );
                for bad in ["様", "さん", "会社", "株式", "店", "病院"] {
                    assert!(!ph.text.contains(bad), "{}", ph.text);
                }
            }
        }
        assert_eq!(n, 27);
    }
}
