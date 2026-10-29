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
    pub opening: String,
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
    pub hidden: Hidden,
    #[serde(default)]
    pub style_notes: Vec<String>,
    #[serde(default)]
    pub phrases: Vec<String>,
}

const RAW: &str = include_str!("data/personas.json");

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
    fn 型は4つで熱い印の数から決まる熱さが手で付けた熱さと一致する() {
        let ids: Vec<&str> = all().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["hot_01", "cold_01", "budget_01", "talkative_01"]);
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
        let raw = RAW;
        assert!(!raw.contains("株式会社") && !raw.contains("@") && !raw.contains("http"));
        let digits_run = raw
            .chars()
            .collect::<Vec<_>>()
            .windows(6)
            .any(|w| w.iter().all(|c| c.is_ascii_digit()));
        assert!(!digits_run);
    }
}
