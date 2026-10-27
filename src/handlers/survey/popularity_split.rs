//! 競合調査「人気の分かれ方」の集計 (Indeed SP の CSV 由来)。
//!
//! 人気 = 「人気」または「超人気」のタグが付いた求人 (合算)。比べる相手 = それ以外。
//! 1 求人は各表で 1 回だけ数える。求人タイトル・見出し文そのものは出力に含めない
//! (会社名と数字、語の有無の割合だけ)。判定や評価の文言は持たない (各行に n を持つ)。
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use ts_rs::TS;

use super::aggregator::{popularity_signal, record_tags};
use super::salary_parser::SalaryType;
use super::upload::{CsvSource, SurveyRecord};

/// 区分ごとの件数。割合は画面で popular / n から計算する。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct Bucket {
    pub label: String,
    pub n: u32,
    pub popular: u32,
    pub super_popular: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct TagStat {
    pub tag: String,
    pub n: u32,
    pub popular: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct AgeTags {
    pub label: String,
    pub tags: Vec<TagStat>,
}

/// 0〜100 の割合。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct TagDiff {
    pub tag: String,
    pub super_pct: f64,
    pub base_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct CredentialShare {
    pub super_pct: f64,
    pub base_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct SuperRecent {
    pub n_super: u32,
    pub n_base: u32,
    pub tags: Vec<TagDiff>,
    pub credential_share: CredentialShare,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct CompanyRow {
    pub name: String,
    pub n: u32,
    pub popular: u32,
    pub super_popular: u32,
    /// 最長の掲載日数 (30 日以上は 30)。掲載日数が読めた求人が無ければ null。
    pub longest_days: Option<u32>,
    /// 30 日以上としか分からない求人を含む。
    pub longest_is_30plus: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct CompanyGroup {
    pub companies: u32,
    pub n: u32,
    pub popular: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct CompanySection {
    /// 求人数の多い順に上位 50 社。
    pub rows: Vec<CompanyRow>,
    pub multi: CompanyGroup,
    pub single: CompanyGroup,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct SalarySection {
    /// 月給下限の中央値 (円/月)。
    pub popular_median: Option<f64>,
    pub popular_n: u32,
    pub other_median: Option<f64>,
    pub other_n: u32,
    /// 月給下限の四分位で 4 帯 + 「月給なし他」。
    pub bands: Vec<Bucket>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct TagSection {
    pub welfare: Vec<TagStat>,
    pub easy_apply: Vec<TagStat>,
    /// 条件タグの数別 (人気・超人気・掲載日数・新着・バッジは数えない)。
    pub count_buckets: Vec<Bucket>,
}

/// 対象の広さ (間口を広げる語の数で分けた帯)。割合は 0〜100。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct ReachBand {
    pub label: String,
    pub n: u32,
    pub popular: u32,
    pub super_popular: u32,
    /// 全求人に占める割合。
    pub n_share_pct: f64,
    /// 人気・超人気全体に占める割合 (絶対数の貢献)。
    pub popular_share_pct: f64,
    /// この帯の人気率。求人 0 件なら None。
    pub rate_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct ReachCell {
    pub n: u32,
    pub popular: u32,
    pub rate_pct: Option<f64>,
}

/// 間口を広げる語以外のタグの数の層ごとの、間口を広げる語の有無の比較。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct ReachStratum {
    pub label: String,
    pub without_open: ReachCell,
    pub with_open: ReachCell,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct ReachTagGap {
    pub tag: String,
    /// 帯の中でこのタグが付く求人数。
    pub n: u32,
    /// 帯の人気求人でのタグの割合 − 帯の全体でのタグの割合 (0〜100 の差)。
    pub diff: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct ReachBandTags {
    pub label: String,
    pub n: u32,
    pub popular: u32,
    /// 求人数が足りない帯は false (表を出さない)。
    pub shown: bool,
    pub more: Vec<ReachTagGap>,
    pub less: Vec<ReachTagGap>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct ReachSection {
    /// 間口を広げる語の一覧。
    pub open_words: Vec<String>,
    /// この求人数に満たない帯は条件タグの差を出さない。
    pub min_band_n: u32,
    pub bands: Vec<ReachBand>,
    pub strata: Vec<ReachStratum>,
    pub band_tags: Vec<ReachBandTags>,
}

/// 会社の影響の判定。決まりの値はここ 1 か所。
const BIAS_MIN_DIFF: f64 = 2.0;
const BIAS_MIN_Z: f64 = 2.0;
/// 判定する会社の最小の掲載数。
const BIAS_MIN_COMPANY_N: u32 = 3;
/// 期待人気率を出す層の最小の求人数 (会社を除いたもの)。
const BIAS_MIN_STRATUM_N: u32 = 10;
const BIAS_COMPANY_ROWS: usize = 20;
const BIAS_TAG_ROWS: usize = 10;
const BIAS_TAG_MIN_N: u32 = 3;
const VERDICT_BIASED: &str = "会社に偏りがある";
const VERDICT_FEWER: &str = "人気が少ない会社";
const VERDICT_RANGE: &str = "給与・条件の範囲";
const VERDICT_NONE: &str = "判定なし";

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct BiasCompanyRow {
    pub name: String,
    pub n: u32,
    pub popular: u32,
    /// 同じ条件のほかの会社の人気率から出した期待人気数。
    pub expected: f64,
    pub diff: f64,
    /// 「会社に偏りがある」「人気が少ない会社」「給与・条件の範囲」「判定なし」。
    pub verdict: String,
}

/// 条件タグごとの 人気求人での割合 − 全体の割合 (百分率の差)。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct BiasTagGap {
    pub tag: String,
    /// 全求人でのタグ付き求人数。
    pub n: u32,
    pub before: f64,
    /// 偏りのある会社を除いた求人での差。除いた後に件数が 3 未満なら null。
    pub after: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct CompanyBias {
    pub total: u32,
    pub popular: u32,
    /// 掲載 3 件以上で判定した会社の数。
    pub judged_companies: u32,
    pub biased_companies: u32,
    pub biased_popular: u32,
    pub biased_popular_pct: f64,
    pub biased_n: u32,
    pub biased_n_pct: f64,
    pub fewer_companies: u32,
    /// 期待人気率を、掲載の長さを外した層・給与帯だけの層・全体の人気率に切り替えた求人数。
    pub fallback_no_age: u32,
    pub fallback_salary_only: u32,
    pub fallback_overall: u32,
    pub rows: Vec<BiasCompanyRow>,
    /// 偏りのある会社を除いた求人の数と人気求人の数。
    pub after_total: u32,
    pub after_popular: u32,
    pub tags_more: Vec<BiasTagGap>,
    pub tags_less: Vec<BiasTagGap>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct Group {
    pub title: String,
    pub rows: Vec<Bucket>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct PopularitySplit {
    pub total: u32,
    pub popular: u32,
    pub super_popular: u32,
    pub by_age: Vec<Bucket>,
    pub tags_by_age: Vec<AgeTags>,
    pub super_recent: Option<SuperRecent>,
    pub companies: CompanySection,
    pub company_bias: CompanyBias,
    pub reach: ReachSection,
    pub salary: SalarySection,
    pub tags: TagSection,
    pub writing: Vec<Group>,
    pub clarity: Vec<Bucket>,
    pub work_style: Vec<Bucket>,
}

/// 間口を広げる語 (応募できる人の範囲を広げるタグ)。変えるときはここだけ。
pub(crate) const OPEN_WORDS: [&str; 5] = [
    "学歴不問",
    "未経験者歓迎",
    "ブランクOK",
    "フリーター歓迎",
    "第二新卒歓迎",
];
const REACH_MIN_BAND_N: u32 = 30;
const REACH_TAG_MIN_N: u32 = 3;
const REACH_TAG_ROWS: usize = 5;
const REACH_BANDS: [&str; 3] = ["なし", "1つ", "2つ以上"];
const REACH_STRATA: [&str; 3] = ["0〜3個", "4〜6個", "7個以上"];

const AGE_FRESH: &str = "0〜14日";
const AGE_MID: &str = "15〜29日";
const AGE_LONG: &str = "30日以上";
const AGE_UNKNOWN: &str = "不明";
const NO_COMPANY: &str = "会社名なし";
const COMPANY_ROWS: usize = 50;
/// タグ表に載せる最小の求人数。
const MIN_TAG_N: u32 = 5;
const AGE_TAG_ROWS: usize = 8;
const SUPER_TAG_ROWS: usize = 10;

const WELFARE_KEYS: [&str; 9] = [
    "休暇",
    "残業",
    "寮",
    "住宅",
    "食事",
    "賞与",
    "昇給",
    "保険",
    "交通費",
];
const EASY_KEYS: [&str; 7] = [
    "学歴",
    "ブランク",
    "第二新卒",
    "フリーター",
    "主婦",
    "研修",
    "未経験",
];
const CREDENTIAL_WORDS: [&str; 4] = ["グループ", "上場", "シェア", "創業"];
const HEADLINE_SYMBOLS: [char; 16] = [
    '★', '☆', '◆', '◇', '■', '□', '●', '○', '◎', '▼', '▲', '【', '】', '♪', '※', '◯',
];

#[derive(Clone, Copy, PartialEq)]
enum Age {
    Fresh,
    Mid,
    Long,
    Unknown,
}

impl Age {
    fn label(self) -> &'static str {
        match self {
            Age::Fresh => AGE_FRESH,
            Age::Mid => AGE_MID,
            Age::Long => AGE_LONG,
            Age::Unknown => AGE_UNKNOWN,
        }
    }
}

fn age_of(r: &SurveyRecord) -> Age {
    match r.posted_days {
        None => Age::Unknown,
        Some(_) if r.posted_plus => Age::Long,
        Some(d) if d <= 14 => Age::Fresh,
        Some(d) if d <= 29 => Age::Mid,
        Some(_) => Age::Long,
    }
}

/// 1 求人の計算済みの値。
struct Item<'a> {
    rec: &'a SurveyRecord,
    popular: bool,
    sup: bool,
    age: Age,
    /// 条件タグ (人気・超人気・掲載日数・新着・数字+ のバッジを除く。重複なし)。
    cond: Vec<String>,
}

pub(crate) fn is_condition_tag(t: &str) -> bool {
    if t == "新着" || t.ends_with("日前") {
        return false;
    }
    let digits = t.strip_suffix('+').unwrap_or(t);
    !(digits.is_empty() || digits.chars().all(|c| c.is_ascii_digit()))
}

/// 会社名の正規化。先頭の空白区切りまでを取り、法人格を除く。空・非公開は「会社名なし」。
fn normalize_company(name: &str) -> String {
    let mut s = name.to_owned();
    for w in ["株式会社", "合同会社", "有限会社"] {
        s = s.replace(w, "");
    }
    let s = s
        .split(|c: char| c.is_whitespace() || c == '\u{3000}')
        .find(|p| !p.is_empty())
        .unwrap_or("")
        .to_owned();
    if s.is_empty() || s.starts_with("非公開") {
        NO_COMPANY.to_owned()
    } else {
        s
    }
}

/// 「年間休日」「年休」などの直後(空白可)に 3 桁以上の数字がある。
fn states_holidays(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        if chars[i] != '年' {
            continue;
        }
        let mut j = i + 1;
        if chars.get(j) == Some(&'間') {
            j += 1;
        }
        if chars.get(j) != Some(&'休') {
            continue;
        }
        j += 1;
        if chars.get(j) == Some(&'日') {
            j += 1;
        }
        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        let digits = chars[j.min(chars.len())..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if digits >= 3 {
            return true;
        }
    }
    false
}

struct Counter {
    order: Vec<String>,
    map: HashMap<String, Bucket>,
}

impl Counter {
    fn new(labels: &[&str]) -> Self {
        Self {
            order: labels.iter().map(|s| (*s).to_owned()).collect(),
            map: HashMap::new(),
        }
    }
    fn add(&mut self, label: &str, it: &Item) {
        let b = self.map.entry(label.to_owned()).or_insert_with(|| Bucket {
            label: label.to_owned(),
            ..Bucket::default()
        });
        b.n += 1;
        b.popular += it.popular as u32;
        b.super_popular += it.sup as u32;
    }
    /// 並び順の通りに返す。`keep_empty` が false の区分は求人が 0 件なら省く。
    fn finish(self, keep_empty: bool) -> Vec<Bucket> {
        let Counter { order, mut map } = self;
        order
            .into_iter()
            .filter_map(|l| match map.remove(&l) {
                Some(b) => Some(b),
                None if keep_empty => Some(Bucket {
                    label: l,
                    ..Bucket::default()
                }),
                None => None,
            })
            .collect()
    }
}

fn median(v: &mut [i64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[m] as f64
    } else {
        (v[m - 1] + v[m]) as f64 / 2.0
    })
}

fn monthly_min(r: &SurveyRecord) -> Option<i64> {
    (r.salary_parsed.salary_type == SalaryType::Monthly)
        .then_some(r.salary_parsed.min_value)
        .flatten()
        .filter(|v| *v > 0)
}

fn tag_stats(items: &[&Item], keys: &[&str]) -> Vec<TagStat> {
    let mut m: BTreeMap<&str, TagStat> = BTreeMap::new();
    for it in items {
        for t in &it.cond {
            if keys.iter().any(|k| t.contains(k)) {
                let e = m.entry(t.as_str()).or_insert_with(|| TagStat {
                    tag: t.clone(),
                    ..TagStat::default()
                });
                e.n += 1;
                e.popular += it.popular as u32;
            }
        }
    }
    let mut v: Vec<TagStat> = m.into_values().filter(|s| s.n >= MIN_TAG_N).collect();
    sort_by_rate(&mut v);
    v
}

/// 人気率の高い順 → 求人数の多い順 → タグ名。
fn sort_by_rate(v: &mut [TagStat]) {
    v.sort_by(|a, b| {
        let ra = a.popular as f64 / a.n as f64;
        let rb = b.popular as f64 / b.n as f64;
        rb.total_cmp(&ra)
            .then(b.n.cmp(&a.n))
            .then_with(|| a.tag.cmp(&b.tag))
    });
}

/// 区分内で人気率が区分全体より高い条件タグの上位。
fn tags_above_rate(items: &[&Item]) -> Vec<TagStat> {
    let n = items.len() as f64;
    if n == 0.0 {
        return Vec::new();
    }
    let base = items.iter().filter(|i| i.popular).count() as f64 / n;
    let mut m: BTreeMap<&str, TagStat> = BTreeMap::new();
    for it in items {
        for t in &it.cond {
            let e = m.entry(t.as_str()).or_insert_with(|| TagStat {
                tag: t.clone(),
                ..TagStat::default()
            });
            e.n += 1;
            e.popular += it.popular as u32;
        }
    }
    let mut v: Vec<TagStat> = m
        .into_values()
        .filter(|s| s.n >= MIN_TAG_N && s.popular as f64 / s.n as f64 > base)
        .collect();
    sort_by_rate(&mut v);
    v.truncate(AGE_TAG_ROWS);
    v
}

fn pct(count: usize, n: usize) -> f64 {
    if n == 0 {
        0.0
    } else {
        count as f64 * 100.0 / n as f64
    }
}

fn super_recent(items: &[Item]) -> Option<SuperRecent> {
    let sup: Vec<&Item> = items
        .iter()
        .filter(|i| i.sup && i.age == Age::Fresh)
        .collect();
    if sup.is_empty() {
        return None;
    }
    let base: Vec<&Item> = items
        .iter()
        .filter(|i| !i.popular && i.age == Age::Fresh)
        .collect();
    let share = |group: &[&Item], tag: &str| {
        pct(
            group
                .iter()
                .filter(|i| i.cond.iter().any(|t| t == tag))
                .count(),
            group.len(),
        )
    };
    let mut all: Vec<&str> = sup
        .iter()
        .chain(base.iter())
        .flat_map(|i| i.cond.iter().map(String::as_str))
        .collect();
    all.sort_unstable();
    all.dedup();
    let mut tags: Vec<TagDiff> = all
        .into_iter()
        .map(|t| TagDiff {
            tag: t.to_owned(),
            super_pct: share(&sup, t),
            base_pct: share(&base, t),
        })
        .filter(|d| d.super_pct > d.base_pct)
        .collect();
    tags.sort_by(|a, b| {
        (b.super_pct - b.base_pct)
            .total_cmp(&(a.super_pct - a.base_pct))
            .then_with(|| a.tag.cmp(&b.tag))
    });
    tags.truncate(SUPER_TAG_ROWS);
    // 見出し (css-lx9x6g) の語の有無だけを見る。文そのものは持ち出さない。
    let has_word = |i: &Item| CREDENTIAL_WORDS.iter().any(|w| i.rec.snippet.contains(w));
    Some(SuperRecent {
        n_super: sup.len() as u32,
        n_base: base.len() as u32,
        tags,
        credential_share: CredentialShare {
            super_pct: pct(sup.iter().filter(|i| has_word(i)).count(), sup.len()),
            base_pct: pct(base.iter().filter(|i| has_word(i)).count(), base.len()),
        },
    })
}

fn companies(items: &[Item]) -> CompanySection {
    let mut by: BTreeMap<String, Vec<&Item>> = BTreeMap::new();
    for it in items {
        by.entry(normalize_company(&it.rec.company_name))
            .or_default()
            .push(it);
    }
    let mut rows: Vec<CompanyRow> = by
        .iter()
        .map(|(name, v)| CompanyRow {
            name: name.clone(),
            n: v.len() as u32,
            popular: v.iter().filter(|i| i.popular).count() as u32,
            super_popular: v.iter().filter(|i| i.sup).count() as u32,
            longest_days: v.iter().filter_map(|i| i.rec.posted_days).max(),
            longest_is_30plus: v.iter().any(|i| i.rec.posted_plus),
        })
        .collect();
    rows.sort_by(|a, b| {
        b.n.cmp(&a.n)
            .then(b.popular.cmp(&a.popular))
            .then_with(|| a.name.cmp(&b.name))
    });
    let group = |multi: bool| {
        let mut g = CompanyGroup::default();
        for (name, v) in &by {
            if name != NO_COMPANY && (v.len() >= 2) == multi {
                g.companies += 1;
                g.n += v.len() as u32;
                g.popular += v.iter().filter(|i| i.popular).count() as u32;
            }
        }
        g
    };
    let (multi, single) = (group(true), group(false));
    rows.truncate(COMPANY_ROWS);
    CompanySection {
        rows,
        multi,
        single,
    }
}

/// 判定の決まり。差と分散から判定名を返す。
fn bias_verdict(diff: f64, variance: f64) -> &'static str {
    if variance <= 1e-12 {
        return VERDICT_NONE;
    }
    let z = diff / variance.sqrt();
    const EPS: f64 = 1e-9;
    if diff >= BIAS_MIN_DIFF - EPS && z >= BIAS_MIN_Z - EPS {
        VERDICT_BIASED
    } else if diff <= -BIAS_MIN_DIFF + EPS && z <= -BIAS_MIN_Z + EPS {
        VERDICT_FEWER
    } else {
        VERDICT_RANGE
    }
}

/// 給与帯の区分 (月給下限の四分位。月給が読めない求人は「不明」)。
fn salary_band_index(items: &[Item]) -> Vec<u8> {
    let mut all: Vec<i64> = items.iter().filter_map(|i| monthly_min(i.rec)).collect();
    all.sort_unstable();
    let q: Vec<i64> = if all.is_empty() {
        Vec::new()
    } else {
        (1..4)
            .map(|k| all[(all.len() * k / 4).min(all.len() - 1)])
            .collect()
    };
    items
        .iter()
        .map(|i| match monthly_min(i.rec) {
            Some(v) => q.iter().filter(|b| v >= **b).count() as u8,
            None => 4,
        })
        .collect()
}

fn count_bucket_index(n: usize) -> u8 {
    match n {
        0 => 0,
        1..=2 => 1,
        3..=4 => 2,
        _ => 3,
    }
}

fn age_index(a: Age) -> u8 {
    match a {
        Age::Fresh => 0,
        Age::Mid => 1,
        Age::Long => 2,
        Age::Unknown => 3,
    }
}

type Tally = (u32, u32);

fn tally_add(m: &mut HashMap<Vec<u32>, Tally>, key: Vec<u32>, popular: bool) {
    let e = m.entry(key).or_default();
    e.0 += 1;
    e.1 += popular as u32;
}

/// 条件タグごとの (全体でのタグ付き求人数, 人気求人での割合 − 全体の割合)。
fn tag_gaps(items: &[&Item]) -> HashMap<String, (u32, f64)> {
    let n = items.len() as f64;
    let pop = items.iter().filter(|i| i.popular).count() as f64;
    let mut m: HashMap<&str, (u32, u32)> = HashMap::new();
    for it in items {
        for t in &it.cond {
            let e = m.entry(t.as_str()).or_default();
            e.0 += 1;
            e.1 += it.popular as u32;
        }
    }
    if pop == 0.0 {
        return HashMap::new();
    }
    m.into_iter()
        .filter(|(_, (c, _))| *c >= BIAS_TAG_MIN_N)
        .map(|(t, (c, p))| (t.to_owned(), (c, (p as f64 / pop - c as f64 / n) * 100.0)))
        .collect()
}

/// 人気が会社によるのか、給与・条件の範囲かを分ける。
/// 各求人の期待人気率 = その求人の会社を除いたほかの会社の、同じ層の人気率。
fn company_bias(items: &[Item]) -> CompanyBias {
    let band = salary_band_index(items);
    // 会社名なしは別々の会社として扱う (一つの会社とは見ない)。
    let comp: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let c = normalize_company(&it.rec.company_name);
            if c == NO_COMPANY {
                format!("\u{0}{i}")
            } else {
                c
            }
        })
        .collect();
    // 層の鍵: [1, 給与帯, タグ数, 掲載] / [2, 給与帯, タグ数] / [3, 給与帯]
    let keys = |i: usize| -> [Vec<u32>; 3] {
        let it = &items[i];
        let (b, c, a) = (
            band[i] as u32,
            count_bucket_index(it.cond.len()) as u32,
            age_index(it.age) as u32,
        );
        [vec![1, b, c, a], vec![2, b, c], vec![3, b]]
    };
    let mut all: [HashMap<Vec<u32>, Tally>; 1] = [HashMap::new()];
    let mut per_company: HashMap<(String, Vec<u32>), Tally> = HashMap::new();
    let mut comp_total: HashMap<&str, Tally> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        for k in keys(i) {
            tally_add(&mut all[0], k.clone(), it.popular);
            let e = per_company.entry((comp[i].clone(), k)).or_default();
            e.0 += 1;
            e.1 += it.popular as u32;
        }
        let e = comp_total.entry(comp[i].as_str()).or_default();
        e.0 += 1;
        e.1 += it.popular as u32;
    }
    let n_all = items.len() as u32;
    let pop_all = items.iter().filter(|i| i.popular).count() as u32;

    let mut fb = (0u32, 0u32, 0u32);
    let mut p_of: Vec<f64> = Vec::with_capacity(items.len());
    for (i, ci) in comp.iter().enumerate() {
        let ks = keys(i);
        let mut chosen = None;
        for (level, k) in ks.iter().enumerate() {
            let (tn, tp) = all[0].get(k).copied().unwrap_or_default();
            let (cn, cp) = per_company
                .get(&(ci.clone(), k.clone()))
                .copied()
                .unwrap_or_default();
            let (on, op) = (tn - cn, tp - cp);
            if on >= BIAS_MIN_STRATUM_N {
                chosen = Some((level, op as f64 / on as f64));
                break;
            }
        }
        let p = match chosen {
            Some((level, p)) => {
                match level {
                    1 => fb.0 += 1,
                    2 => fb.1 += 1,
                    _ => {}
                }
                p
            }
            None => {
                fb.2 += 1;
                let (cn, cp) = comp_total.get(ci.as_str()).copied().unwrap_or_default();
                let (on, op) = (n_all - cn, pop_all - cp);
                if on > 0 {
                    op as f64 / on as f64
                } else {
                    pop_all as f64 / n_all as f64
                }
            }
        };
        p_of.push(p);
    }

    let mut rows: Vec<BiasCompanyRow> = Vec::new();
    let mut by: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, c) in comp.iter().enumerate() {
        by.entry(c.as_str()).or_default().push(i);
    }
    for (name, idx) in &by {
        if (idx.len() as u32) < BIAS_MIN_COMPANY_N {
            continue;
        }
        let popular = idx.iter().filter(|i| items[**i].popular).count() as u32;
        let expected: f64 = idx.iter().map(|i| p_of[*i]).sum();
        let variance: f64 = idx.iter().map(|i| p_of[*i] * (1.0 - p_of[*i])).sum();
        let diff = popular as f64 - expected;
        rows.push(BiasCompanyRow {
            name: (*name).to_owned(),
            n: idx.len() as u32,
            popular,
            expected,
            diff,
            verdict: bias_verdict(diff, variance).to_owned(),
        });
    }
    let judged_companies = rows.len() as u32;
    let biased: Vec<&BiasCompanyRow> = rows
        .iter()
        .filter(|r| r.verdict == VERDICT_BIASED)
        .collect();
    let biased_names: std::collections::HashSet<&str> =
        biased.iter().map(|r| r.name.as_str()).collect();
    let biased_companies = biased.len() as u32;
    let biased_popular: u32 = biased.iter().map(|r| r.popular).sum();
    let biased_n: u32 = biased.iter().map(|r| r.n).sum();
    let fewer_companies = rows.iter().filter(|r| r.verdict == VERDICT_FEWER).count() as u32;

    let before_refs: Vec<&Item> = items.iter().collect();
    let after_refs: Vec<&Item> = items
        .iter()
        .enumerate()
        .filter(|(i, _)| !biased_names.contains(comp[*i].as_str()))
        .map(|(_, it)| it)
        .collect();
    let before = tag_gaps(&before_refs);
    let after = tag_gaps(&after_refs);
    let mut gaps: Vec<BiasTagGap> = before
        .into_iter()
        .map(|(tag, (n, b))| BiasTagGap {
            after: after.get(&tag).map(|a| a.1),
            tag,
            n,
            before: b,
        })
        .collect();
    gaps.sort_by(|a, b| {
        b.before
            .total_cmp(&a.before)
            .then_with(|| a.tag.cmp(&b.tag))
    });
    let tags_more: Vec<BiasTagGap> = gaps
        .iter()
        .filter(|g| g.before > 0.0)
        .take(BIAS_TAG_ROWS)
        .cloned()
        .collect();
    let tags_less: Vec<BiasTagGap> = gaps
        .iter()
        .rev()
        .filter(|g| g.before < 0.0)
        .take(BIAS_TAG_ROWS)
        .cloned()
        .collect();

    rows.sort_by(|a, b| {
        ((b.verdict == VERDICT_BIASED) as u8)
            .cmp(&((a.verdict == VERDICT_BIASED) as u8))
            .then(b.diff.total_cmp(&a.diff))
            .then_with(|| a.name.cmp(&b.name))
    });
    rows.truncate(BIAS_COMPANY_ROWS);

    CompanyBias {
        total: n_all,
        popular: pop_all,
        judged_companies,
        biased_companies,
        biased_popular,
        biased_popular_pct: pct(biased_popular as usize, pop_all as usize),
        biased_n,
        biased_n_pct: pct(biased_n as usize, n_all as usize),
        fewer_companies,
        fallback_no_age: fb.0,
        fallback_salary_only: fb.1,
        fallback_overall: fb.2,
        rows,
        after_total: after_refs.len() as u32,
        after_popular: after_refs.iter().filter(|i| i.popular).count() as u32,
        tags_more,
        tags_less,
    }
}

fn salary(items: &[Item]) -> SalarySection {
    let mut all: Vec<i64> = items.iter().filter_map(|i| monthly_min(i.rec)).collect();
    all.sort_unstable();
    let q: Vec<i64> = if all.is_empty() {
        Vec::new()
    } else {
        (1..4)
            .map(|k| all[(all.len() * k / 4).min(all.len() - 1)])
            .collect()
    };
    let man = |v: i64| format!("{:.1}万", v as f64 / 10000.0);
    let labels: Vec<String> = if all.is_empty() {
        Vec::new()
    } else {
        vec![
            format!("〜{}", man(q[0])),
            format!("{}〜{}", man(q[0]), man(q[1])),
            format!("{}〜{}", man(q[1]), man(q[2])),
            format!("{}〜", man(q[2])),
        ]
    };
    let none = "月給なし他";
    let mut order: Vec<&str> = labels.iter().map(String::as_str).collect();
    order.push(none);
    let mut c = Counter::new(&order);
    for it in items {
        match monthly_min(it.rec) {
            Some(v) => {
                let i = q.iter().filter(|b| v >= **b).count();
                c.add(&labels[i], it);
            }
            None => c.add(none, it),
        }
    }
    let mut p: Vec<i64> = items
        .iter()
        .filter(|i| i.popular)
        .filter_map(|i| monthly_min(i.rec))
        .collect();
    let mut o: Vec<i64> = items
        .iter()
        .filter(|i| !i.popular)
        .filter_map(|i| monthly_min(i.rec))
        .collect();
    SalarySection {
        popular_n: p.len() as u32,
        other_n: o.len() as u32,
        popular_median: median(&mut p),
        other_median: median(&mut o),
        bands: c.finish(true),
    }
}

fn cell(items: &[&Item]) -> ReachCell {
    let n = items.len() as u32;
    let popular = items.iter().filter(|i| i.popular).count() as u32;
    ReachCell {
        n,
        popular,
        rate_pct: (n > 0).then(|| pct(popular as usize, n as usize)),
    }
}

fn open_count(it: &Item) -> usize {
    it.cond
        .iter()
        .filter(|t| OPEN_WORDS.contains(&t.as_str()))
        .count()
}

fn band_index(open: usize) -> usize {
    open.min(2)
}

/// 帯の中で、人気求人での割合 − 全体の割合 が大きい / 小さい条件タグ (間口を広げる語は除く)。
fn reach_tag_gaps(items: &[&Item]) -> (Vec<ReachTagGap>, Vec<ReachTagGap>) {
    let n = items.len();
    let pop = items.iter().filter(|i| i.popular).count();
    if n == 0 || pop == 0 {
        return (Vec::new(), Vec::new());
    }
    let mut m: BTreeMap<&str, (u32, u32)> = BTreeMap::new();
    for it in items {
        for t in &it.cond {
            if OPEN_WORDS.contains(&t.as_str()) {
                continue;
            }
            let e = m.entry(t.as_str()).or_default();
            e.0 += 1;
            e.1 += it.popular as u32;
        }
    }
    let mut v: Vec<ReachTagGap> = m
        .into_iter()
        .filter(|(_, (c, _))| *c >= REACH_TAG_MIN_N)
        .map(|(t, (c, p))| ReachTagGap {
            tag: t.to_owned(),
            n: c,
            diff: pct(p as usize, pop) - pct(c as usize, n),
        })
        .collect();
    v.sort_by(|a, b| {
        b.diff
            .total_cmp(&a.diff)
            .then(b.n.cmp(&a.n))
            .then_with(|| a.tag.cmp(&b.tag))
    });
    let more: Vec<ReachTagGap> = v
        .iter()
        .filter(|g| g.diff > 0.0)
        .take(REACH_TAG_ROWS)
        .cloned()
        .collect();
    let mut less: Vec<ReachTagGap> = v
        .into_iter()
        .rev()
        .filter(|g| g.diff < 0.0)
        .take(REACH_TAG_ROWS)
        .collect();
    less.sort_by(|a, b| {
        a.diff
            .total_cmp(&b.diff)
            .then(b.n.cmp(&a.n))
            .then_with(|| a.tag.cmp(&b.tag))
    });
    (more, less)
}

fn reach(items: &[Item]) -> ReachSection {
    let total = items.len();
    let popular = items.iter().filter(|i| i.popular).count();
    let mut by_band: [Vec<&Item>; 3] = Default::default();
    let mut strata: [[Vec<&Item>; 2]; 3] = Default::default();
    for it in items {
        let open = open_count(it);
        by_band[band_index(open)].push(it);
        let others = it.cond.len() - open;
        let s = match others {
            0..=3 => 0,
            4..=6 => 1,
            _ => 2,
        };
        strata[s][(open > 0) as usize].push(it);
    }
    let bands = by_band
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let c = cell(v);
            ReachBand {
                label: REACH_BANDS[i].to_owned(),
                n: c.n,
                popular: c.popular,
                super_popular: v.iter().filter(|x| x.sup).count() as u32,
                n_share_pct: pct(v.len(), total),
                popular_share_pct: pct(c.popular as usize, popular),
                rate_pct: c.rate_pct,
            }
        })
        .collect();
    let strata = strata
        .iter()
        .enumerate()
        .map(|(i, [w, o])| ReachStratum {
            label: REACH_STRATA[i].to_owned(),
            without_open: cell(w),
            with_open: cell(o),
        })
        .collect();
    let band_tags = by_band
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let c = cell(v);
            let shown = c.n >= REACH_MIN_BAND_N;
            let (more, less) = if shown {
                reach_tag_gaps(v)
            } else {
                (Vec::new(), Vec::new())
            };
            ReachBandTags {
                label: REACH_BANDS[i].to_owned(),
                n: c.n,
                popular: c.popular,
                shown,
                more,
                less,
            }
        })
        .collect();
    ReachSection {
        open_words: OPEN_WORDS.iter().map(|w| (*w).to_owned()).collect(),
        min_band_n: REACH_MIN_BAND_N,
        bands,
        strata,
        band_tags,
    }
}

/// 人気の分かれ方を集計する。Indeed (SP) 由来の求人が無ければ None。
pub fn compute(records: &[SurveyRecord]) -> Option<PopularitySplit> {
    let items: Vec<Item> = records
        .iter()
        .filter(|r| matches!(r.source, CsvSource::IndeedSp))
        .map(|rec| {
            let (sup, pop) = popularity_signal(&rec.tags_raw);
            Item {
                rec,
                popular: sup || pop,
                sup,
                age: age_of(rec),
                cond: record_tags(&rec.tags_raw)
                    .into_iter()
                    .filter(|t| is_condition_tag(t))
                    .collect(),
            }
        })
        .collect();
    if items.is_empty() {
        return None;
    }

    let mut age = Counter::new(&[AGE_FRESH, AGE_MID, AGE_LONG, AGE_UNKNOWN]);
    let mut counts = Counter::new(&["0個", "1〜2個", "3〜4個", "5個以上"]);
    let mut len = Counter::new(&["見出しなし", "〜25字", "26〜40字", "41字〜"]);
    let mut sym = Counter::new(&["見出し記号あり", "見出し記号なし"]);
    let mut hol = Counter::new(&["休日日数の明記あり", "休日日数の明記なし"]);
    let mut clarity = Counter::new(&["0点", "1点", "2点", "3点", "4点"]);
    let mut style = Counter::new(&["夜勤", "シフト制", "固定・不明"]);
    for it in &items {
        age.add(it.age.label(), it);
        counts.add(
            match it.cond.len() {
                0 => "0個",
                1..=2 => "1〜2個",
                3..=4 => "3〜4個",
                _ => "5個以上",
            },
            it,
        );
        let head = it.rec.snippet.trim();
        let hl = head.chars().count();
        len.add(
            match hl {
                0 => "見出しなし",
                1..=25 => "〜25字",
                26..=40 => "26〜40字",
                _ => "41字〜",
            },
            it,
        );
        let has_sym = head.chars().any(|c| HEADLINE_SYMBOLS.contains(&c));
        sym.add(
            if has_sym {
                "見出し記号あり"
            } else {
                "見出し記号なし"
            },
            it,
        );
        let text = format!("{} {} {}", head, it.rec.job_title, it.rec.tags_raw);
        let holiday = states_holidays(&text);
        hol.add(
            if holiday {
                "休日日数の明記あり"
            } else {
                "休日日数の明記なし"
            },
            it,
        );
        let points = (hl > 0) as usize
            + it.rec.salary_raw.chars().any(|c| c.is_numeric()) as usize
            + (it.cond.len() >= 3) as usize
            + holiday as usize;
        clarity.add(["0点", "1点", "2点", "3点", "4点"][points], it);
        let night = head.contains("夜勤") || it.rec.job_title.contains("夜勤");
        style.add(
            if night {
                "夜勤"
            } else if it.cond.iter().any(|t| t.contains("シフト制")) {
                "シフト制"
            } else {
                "固定・不明"
            },
            it,
        );
    }
    let in_age = |a: Age| -> Vec<&Item> { items.iter().filter(|i| i.age == a).collect() };
    let all_refs: Vec<&Item> = items.iter().collect();
    let mut clarity = clarity.finish(false);
    // 0 点が無いときは先頭の空行を作らない (finish は 0 件の区分を省く)。
    clarity.retain(|b| b.n > 0);
    Some(PopularitySplit {
        total: items.len() as u32,
        popular: items.iter().filter(|i| i.popular).count() as u32,
        super_popular: items.iter().filter(|i| i.sup).count() as u32,
        by_age: age.finish(true),
        tags_by_age: vec![
            AgeTags {
                label: AGE_FRESH.to_owned(),
                tags: tags_above_rate(&in_age(Age::Fresh)),
            },
            AgeTags {
                label: AGE_MID.to_owned(),
                tags: tags_above_rate(&in_age(Age::Mid)),
            },
            AgeTags {
                label: AGE_LONG.to_owned(),
                tags: tags_above_rate(&in_age(Age::Long)),
            },
        ],
        super_recent: super_recent(&items),
        companies: companies(&items),
        company_bias: company_bias(&items),
        reach: reach(&items),
        salary: salary(&items),
        tags: TagSection {
            welfare: tag_stats(&all_refs, &WELFARE_KEYS),
            easy_apply: tag_stats(&all_refs, &EASY_KEYS),
            count_buckets: counts.finish(true),
        },
        writing: vec![
            Group {
                title: "見出しの長さ".to_owned(),
                rows: len.finish(true),
            },
            Group {
                title: "見出し記号".to_owned(),
                rows: sym.finish(true),
            },
            Group {
                title: "休日日数の明記".to_owned(),
                rows: hol.finish(true),
            },
        ],
        clarity,
        work_style: style.finish(true),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::survey::salary_parser::parse_salary;
    use crate::handlers::survey::upload::{
        parse_csv_bytes_with_hints, parse_posted_age, UserSourceHint,
    };

    /// 合成レコード。タグ列は tags_raw にそのまま入れる。
    fn rec(
        company: &str,
        snippet: &str,
        salary: &str,
        tags: &str,
        age: Option<(u32, bool)>,
    ) -> SurveyRecord {
        let mut r = parse_csv_bytes_with_hints(
            "css-bxyec3,css-14qk2ra,css-18rxko3,css-u74ql7\nタイトル,会社,月給20万円,人気\n"
                .as_bytes(),
            None,
            UserSourceHint::Auto,
        )
        .unwrap()
        .remove(0);
        r.source = CsvSource::IndeedSp;
        r.job_title = "秘密のタイトル".to_owned();
        r.company_name = company.to_owned();
        r.snippet = snippet.to_owned();
        r.salary_raw = salary.to_owned();
        r.salary_parsed = parse_salary(salary, SalaryType::Monthly);
        r.tags_raw = tags.to_owned();
        r.posted_days = age.map(|a| a.0);
        r.posted_plus = age.is_some_and(|a| a.1);
        r
    }

    fn bucket<'a>(v: &'a [Bucket], label: &str) -> &'a Bucket {
        v.iter().find(|b| b.label == label).unwrap()
    }

    #[test]
    fn posted_age_parsing() {
        assert_eq!(
            parse_posted_age(["x", "14日前"].into_iter()),
            (Some(14), false)
        );
        assert_eq!(
            parse_posted_age(["30+日前", "新着"].into_iter()),
            (Some(30), true)
        );
        assert_eq!(parse_posted_age(["", "新着"].into_iter()), (Some(0), false));
        assert_eq!(parse_posted_age(["3日前の求人"].into_iter()), (None, false));
        assert_eq!(parse_posted_age(["+日前"].into_iter()), (None, false));
    }

    #[test]
    fn age_boundaries_and_single_counting() {
        let recs = vec![
            rec("A", "", "月給20万円", "人気", Some((14, false))),
            rec("B", "", "月給20万円", "超人気,人気", Some((0, false))),
            rec("C", "", "月給20万円", "", Some((15, false))),
            rec("D", "", "月給20万円", "人気", Some((29, false))),
            rec("E", "", "月給20万円", "超人気", Some((30, true))),
            rec("F", "", "月給20万円", "", Some((45, false))),
            rec("G", "", "月給20万円", "", None),
        ];
        let s = compute(&recs).unwrap();
        assert_eq!((s.total, s.popular, s.super_popular), (7, 4, 2));
        let f = bucket(&s.by_age, AGE_FRESH);
        assert_eq!((f.n, f.popular, f.super_popular), (2, 2, 1));
        let m = bucket(&s.by_age, AGE_MID);
        assert_eq!((m.n, m.popular, m.super_popular), (2, 1, 0));
        let l = bucket(&s.by_age, AGE_LONG);
        assert_eq!((l.n, l.popular, l.super_popular), (2, 1, 1));
        let u = bucket(&s.by_age, AGE_UNKNOWN);
        assert_eq!((u.n, u.popular), (1, 0));
        // 超人気かつ人気の求人 B は 1 回だけ数える
        assert_eq!(s.by_age.iter().map(|b| b.n).sum::<u32>(), 7);
        // 条件タグの比較は 0〜14日・15〜29日・30日以上の3区分
        let labels: Vec<&str> = s.tags_by_age.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, vec![AGE_FRESH, AGE_MID, AGE_LONG]);
    }

    #[test]
    fn super_recent_only_when_fresh_super_exists() {
        let none = vec![
            rec("A", "", "月給20万円", "超人気", Some((30, true))),
            rec("B", "", "月給20万円", "人気", Some((3, false))),
        ];
        assert!(compute(&none).unwrap().super_recent.is_none());
        let some = vec![
            rec(
                "A",
                "上場企業グループ",
                "月給20万円",
                "超人気,学歴不問",
                Some((3, false)),
            ),
            rec("B", "未経験", "月給20万円", "学歴不問", Some((5, false))),
            rec("C", "安定", "月給20万円", "", Some((7, false))),
            rec("D", "x", "月給20万円", "人気", Some((7, false))),
        ];
        let sr = compute(&some).unwrap().super_recent.unwrap();
        assert_eq!((sr.n_super, sr.n_base), (1, 2));
        assert_eq!(sr.tags.len(), 1);
        assert_eq!(sr.tags[0].tag, "学歴不問");
        assert_eq!((sr.tags[0].super_pct, sr.tags[0].base_pct), (100.0, 50.0));
        assert_eq!(
            (sr.credential_share.super_pct, sr.credential_share.base_pct),
            (100.0, 0.0)
        );
    }

    #[test]
    fn company_normalization_and_groups() {
        assert_eq!(normalize_company("株式会社ミナト 本社"), "ミナト");
        assert_eq!(normalize_company("合同会社 ソラ"), "ソラ");
        assert_eq!(normalize_company("ハナ有限会社"), "ハナ");
        assert_eq!(normalize_company("  "), NO_COMPANY);
        assert_eq!(normalize_company("非公開"), NO_COMPANY);
        let recs = vec![
            rec(
                "株式会社ミナト",
                "",
                "月給20万円",
                "超人気",
                Some((30, true)),
            ),
            rec("ミナト 横浜", "", "月給20万円", "", Some((9, false))),
            rec("ソラ", "", "月給20万円", "人気", Some((2, false))),
            rec("", "", "月給20万円", "", None),
        ];
        let c = compute(&recs).unwrap().companies;
        let minato = &c.rows[0];
        assert_eq!(minato.name, "ミナト");
        assert_eq!((minato.n, minato.popular, minato.super_popular), (2, 1, 1));
        assert_eq!(
            (minato.longest_days, minato.longest_is_30plus),
            (Some(30), true)
        );
        assert_eq!((c.multi.companies, c.multi.n, c.multi.popular), (1, 2, 1));
        // 「会社名なし」は 2 件以上/1 件の集計に入れない
        assert_eq!(
            (c.single.companies, c.single.n, c.single.popular),
            (1, 1, 1)
        );
        assert!(c.rows.iter().any(|r| r.name == NO_COMPANY));
    }

    #[test]
    fn writing_clarity_and_work_style() {
        let recs = vec![
            rec(
                "A",
                "【夜勤】年間休日125日",
                "月給20万円",
                "研修あり,学歴不問,未経験歓迎,人気",
                Some((1, false)),
            ),
            rec("B", "", "応相談", "シフト制", Some((1, false))),
            rec("C", "短い見出し", "月給20万円", "", Some((1, false))),
        ];
        let s = compute(&recs).unwrap();
        // A: 見出し+給与+タグ3+休日 = 4 点、B: 0 点、C: 見出し+給与 = 2 点
        assert_eq!(bucket(&s.clarity, "4点").n, 1);
        assert_eq!(bucket(&s.clarity, "4点").popular, 1);
        assert_eq!(bucket(&s.clarity, "0点").n, 1);
        assert_eq!(bucket(&s.clarity, "2点").n, 1);
        assert!(s
            .clarity
            .iter()
            .all(|b| b.label != "1点" && b.label != "3点"));
        assert_eq!(bucket(&s.work_style, "夜勤").n, 1);
        assert_eq!(bucket(&s.work_style, "シフト制").n, 1);
        assert_eq!(bucket(&s.work_style, "固定・不明").n, 1);
        let g = |t: &str| &s.writing.iter().find(|g| g.title == t).unwrap().rows;
        assert_eq!(bucket(g("見出しの長さ"), "見出しなし").n, 1);
        assert_eq!(bucket(g("見出しの長さ"), "〜25字").n, 2);
        assert_eq!(bucket(g("見出し記号"), "見出し記号あり").n, 1);
        assert_eq!(bucket(g("休日日数の明記"), "休日日数の明記あり").n, 1);
        // 条件タグ数: A=3 個 (人気は数えない)
        assert_eq!(bucket(&s.tags.count_buckets, "3〜4個").n, 1);
        assert_eq!(bucket(&s.tags.count_buckets, "0個").n, 1);
        assert!(states_holidays("年休 120日"));
        assert!(!states_holidays("年休12日"));
    }

    #[test]
    fn salary_bands_and_medians() {
        let recs: Vec<SurveyRecord> = (0..8)
            .map(|i| {
                let man = 20 + i;
                rec(
                    "A",
                    "",
                    &format!("月給{man}万円"),
                    if i >= 6 { "人気" } else { "" },
                    None,
                )
            })
            .chain(std::iter::once(rec("A", "", "応相談", "", None)))
            .collect();
        let s = compute(&recs).unwrap().salary;
        // 下限 20..27 万円の 8 件: 四分位境界は 22 / 24 / 26 万円
        let counts: Vec<u32> = s.bands.iter().map(|b| b.n).collect();
        assert_eq!(counts, vec![2, 2, 2, 2, 1]);
        assert_eq!(s.bands[0].label, "〜22.0万");
        assert_eq!(s.bands[4].label, "月給なし他");
        assert_eq!(s.bands[3].popular, 2);
        assert_eq!((s.popular_n, s.popular_median), (2, Some(265000.0)));
        assert_eq!((s.other_n, s.other_median), (6, Some(225000.0)));
    }

    #[test]
    fn json_has_no_job_titles() {
        let recs = vec![rec("A", "見出し", "月給20万円", "人気", Some((1, false)))];
        let json = serde_json::to_string(&compute(&recs).unwrap()).unwrap();
        assert!(!json.contains("秘密のタイトル"));
        assert!(!json.contains("見出し\""));
    }

    #[test]
    fn none_without_indeed_sp() {
        let mut r = rec("A", "", "月給20万円", "人気", None);
        r.source = CsvSource::Indeed;
        assert!(compute(&[r]).is_none());
    }

    /// 実 CSV がある環境でだけ走らせる (cargo test --lib real_csv -- --ignored)。
    #[test]
    #[ignore]
    fn real_csv_2026_10_26_age_split() {
        let path = "/Users/s_fujimaki/Downloads/indeed-2026-10-26.csv";
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let recs = parse_csv_bytes_with_hints(&bytes, None, UserSourceHint::Auto).unwrap();
        let s = compute(&recs).unwrap();
        for b in &s.by_age {
            eprintln!(
                "{} n={} pop={} sup={}",
                b.label, b.n, b.popular, b.super_popular
            );
        }
        eprintln!("total={} popular={}", s.total, s.popular);
        for b in &s.tags.count_buckets {
            eprintln!("tags {} n={} pop={}", b.label, b.n, b.popular);
        }
        let g = |l: &str| {
            s.by_age
                .iter()
                .find(|b| b.label == l)
                .map(|b| (b.n, b.popular))
                .unwrap()
        };
        // CSV は 325 行。取り込み時の重複排除 (同じ求人キーの再収集) で 15〜29日の 2 件と 30日以上の 1 件が
        // 除かれ、人気 1 件が減る。重複排除前の生の行数は 82/19、61/30、182/57 (計 325、人気 106)。
        assert_eq!(g(AGE_FRESH), (82, 19));
        assert_eq!(g(AGE_MID), (59, 29));
        assert_eq!(g(AGE_LONG), (181, 57));
        assert_eq!((s.total, s.popular), (322, 105));
    }

    // ---- 会社の影響 ----

    fn repeat(c: &str, tags: &str, age: Option<(u32, bool)>, n: usize) -> Vec<SurveyRecord> {
        (0..n)
            .map(|_| rec(c, "", "月給20万円", tags, age))
            .collect()
    }

    /// 会社 X の 6 求人が全部人気。ほかの 15 社 (2 件ずつ) は 30 件中 3 件が人気。
    fn concentrated() -> Vec<SurveyRecord> {
        let a = Some((1, false));
        let mut v = repeat("X", "人気,寮完備", a, 6);
        for i in 1..=15 {
            let c = format!("O{i}");
            let first = match i {
                1..=3 => "人気,昇給あり",
                4..=6 => "昇給あり",
                _ => "",
            };
            v.push(rec(&c, "", "月給20万円", first, a));
            v.push(rec(&c, "", "月給20万円", "", a));
        }
        v
    }

    #[test]
    fn verdict_boundaries() {
        // 差ちょうど 2、z ちょうど 2 (分散 1) は偏りあり
        assert_eq!(bias_verdict(2.0, 1.0), VERDICT_BIASED);
        // z が 2 に届かない
        assert_eq!(bias_verdict(2.0, 1.01), VERDICT_RANGE);
        // 差が 2 に届かない
        assert_eq!(bias_verdict(1.99, 0.25), VERDICT_RANGE);
        assert_eq!(bias_verdict(-2.0, 1.0), VERDICT_FEWER);
        assert_eq!(bias_verdict(-1.99, 0.25), VERDICT_RANGE);
        assert_eq!(bias_verdict(5.0, 0.0), VERDICT_NONE);
    }

    #[test]
    fn concentrated_popularity_is_company_bias() {
        let s = compute(&concentrated()).unwrap();
        let b = &s.company_bias;
        assert_eq!((b.total, b.popular), (36, 9));
        // 掲載 3 件未満の会社は出ない
        assert_eq!(b.rows.len(), 1);
        assert_eq!(b.judged_companies, 1);
        let x = &b.rows[0];
        assert_eq!((x.name.as_str(), x.n, x.popular), ("X", 6, 6));
        // 期待人気数は X を除いた 30 件の人気率 (3/30) から。X を含めると 1.5 になってしまう
        assert!((x.expected - 0.6).abs() < 1e-9, "{}", x.expected);
        assert!((x.diff - 5.4).abs() < 1e-9);
        assert_eq!(x.verdict, VERDICT_BIASED);
        assert_eq!(b.biased_companies, 1);
        assert_eq!((b.biased_popular, b.biased_n), (6, 6));
        assert!((b.biased_popular_pct - 100.0 * 6.0 / 9.0).abs() < 1e-9);
        assert!((b.biased_n_pct - 100.0 * 6.0 / 36.0).abs() < 1e-9);
        assert_eq!((b.after_total, b.after_popular), (30, 3));
    }

    #[test]
    fn tag_gap_before_and_after_removing_biased_company() {
        let b = compute(&concentrated()).unwrap().company_bias;
        let get = |v: &Vec<BiasTagGap>, t: &str| v.iter().find(|g| g.tag == t).cloned();
        // 寮完備は X だけが持つ: 6/9 − 6/36 = 50 pt。X を除くと求人が無くなる
        let ryo = get(&b.tags_more, "寮完備").unwrap();
        assert_eq!(ryo.n, 6);
        assert!((ryo.before - 50.0).abs() < 1e-9);
        assert_eq!(ryo.after, None);
        // 昇給あり: 3/9 − 6/36 = 16.67 pt、X を除くと 3/3 − 6/30 = 80 pt
        let kyu = get(&b.tags_more, "昇給あり").unwrap();
        assert!((kyu.before - (100.0 / 3.0 - 100.0 / 6.0)).abs() < 1e-9);
        assert!((kyu.after.unwrap() - 80.0).abs() < 1e-9);
    }

    #[test]
    fn same_rate_everywhere_has_no_bias() {
        let a = Some((1, false));
        let mut v = Vec::new();
        for c in ["A", "B", "C"] {
            v.extend(repeat(c, "人気", a, 2));
            v.extend(repeat(c, "", a, 2));
        }
        let b = compute(&v).unwrap().company_bias;
        assert_eq!(b.biased_companies, 0);
        assert_eq!(b.rows.len(), 3);
        for r in &b.rows {
            assert!((r.expected - 2.0).abs() < 1e-9);
            assert_eq!(r.verdict, VERDICT_RANGE);
        }
    }

    #[test]
    fn stratum_fallback_levels() {
        let fresh = Some((1, false));
        let long = Some((40, false));
        let mut v = repeat("A", "", fresh, 3);
        v.extend(repeat("B", "休暇あり,賞与あり,昇給あり", long, 3));
        for i in 1..=6 {
            let c = format!("O{i}");
            v.push(rec(&c, "", "月給20万円", "人気", long));
            v.push(rec(&c, "", "月給20万円", "", long));
        }
        let b = compute(&v).unwrap().company_bias;
        // A は掲載の長さが同じ層にほかの会社が無く、掲載の長さを外した層へ
        assert_eq!(b.fallback_no_age, 3);
        // B はタグ数が同じ層にほかの会社が無く、給与帯だけの層へ
        assert_eq!(b.fallback_salary_only, 3);
        assert_eq!(b.fallback_overall, 0);
        // A の期待人気率は A 以外の「タグ 0 個」12 件の 6/12
        let a = b.rows.iter().find(|r| r.name == "A").unwrap();
        assert!((a.expected - 1.5).abs() < 1e-9, "{}", a.expected);
        // B は A を除く給与帯の層: B 以外 15 件中 6 件が人気
        let bb = b.rows.iter().find(|r| r.name == "B").unwrap();
        assert!(
            (bb.expected - 3.0 * 6.0 / 15.0).abs() < 1e-9,
            "{}",
            bb.expected
        );
    }

    #[test]
    fn small_data_falls_back_to_overall() {
        let a = Some((1, false));
        let mut v = repeat("A", "人気", a, 3);
        v.extend(repeat("B", "", a, 3));
        let b = compute(&v).unwrap().company_bias;
        assert_eq!(b.fallback_overall, 6);
        let ra = b.rows.iter().find(|r| r.name == "A").unwrap();
        assert!((ra.expected - 0.0).abs() < 1e-9);
    }

    #[test]
    fn bias_json_has_no_titles() {
        let json = serde_json::to_string(&compute(&concentrated()).unwrap().company_bias).unwrap();
        assert!(!json.contains("秘密のタイトル"));
    }

    /// 実 CSV の確認 (cargo test --lib real_csv_company_bias -- --ignored --nocapture)。
    #[test]
    #[ignore]
    fn real_csv_company_bias() {
        let path = "/Users/s_fujimaki/Downloads/indeed-2026-10-26.csv";
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let recs = parse_csv_bytes_with_hints(&bytes, None, UserSourceHint::Auto).unwrap();
        let b = compute(&recs).unwrap().company_bias;
        eprintln!("{b:#?}");
        assert_eq!((b.total, b.popular), (322, 105));
    }

    // ---- 対象の広さ ----

    fn many(tags: &str, n: usize) -> Vec<SurveyRecord> {
        (0..n)
            .map(|_| rec("A", "", "月給20万円", tags, Some((1, false))))
            .collect()
    }

    #[test]
    fn reach_band_boundaries_and_shares() {
        // 0 語: 4 件 (人気 1)、1 語: 3 件 (人気 1)、2 語: 2 件 (人気 2)、3 語: 1 件 (超人気)
        let mut v = many("休暇あり", 3);
        v.extend(many("人気,休暇あり", 1));
        v.extend(many("学歴不問", 2));
        v.extend(many("人気,学歴不問", 1));
        v.extend(many("人気,学歴不問,ブランクOK", 1));
        v.extend(many("超人気,学歴不問,ブランクOK", 1));
        v.extend(many("超人気,学歴不問,ブランクOK,未経験者歓迎", 1));
        let r = compute(&v).unwrap().reach;
        assert_eq!(r.bands.len(), 3);
        let b: Vec<_> = r
            .bands
            .iter()
            .map(|x| (x.label.as_str(), x.n, x.popular, x.super_popular))
            .collect();
        assert_eq!(
            b,
            vec![("なし", 4, 1, 0), ("1つ", 3, 1, 0), ("2つ以上", 3, 3, 2)]
        );
        assert!((r.bands[0].n_share_pct - 40.0).abs() < 1e-9);
        assert!((r.bands[2].popular_share_pct - 60.0).abs() < 1e-9);
        assert_eq!(r.bands[2].rate_pct, Some(100.0));
        // 人気・超人気・新着は条件タグに数えない: 1 求人 1 回
        assert_eq!(r.open_words.len(), 5);
    }

    #[test]
    fn reach_without_popular_has_no_gaps_and_zero_share() {
        let v = many("休暇あり,昇給あり", 40);
        let r = compute(&v).unwrap().reach;
        assert_eq!(r.bands[0].popular, 0);
        assert_eq!(r.bands[0].popular_share_pct, 0.0);
        assert!(r.band_tags[0].shown);
        assert!(r.band_tags[0].more.is_empty() && r.band_tags[0].less.is_empty());
        assert_eq!(r.bands[1].n, 0);
        assert_eq!(r.bands[1].rate_pct, None);
    }

    #[test]
    fn reach_strata_counts_exclude_open_words() {
        // 他タグ 4 個 + 間口 1 語 → 4〜6 層の「あり」。他タグ 0 個で間口 0 → 0〜3 層の「なし」。
        let mut v = many("人気,休暇あり,昇給あり,賞与あり,保険あり,学歴不問", 2);
        v.extend(many("休暇あり,昇給あり,賞与あり,保険あり,学歴不問", 2));
        v.extend(many("", 3));
        let r = compute(&v).unwrap().reach;
        let s0 = &r.strata[0];
        assert_eq!((s0.without_open.n, s0.without_open.popular), (3, 0));
        assert_eq!(s0.with_open.n, 0);
        assert_eq!(s0.with_open.rate_pct, None);
        let s1 = &r.strata[1];
        assert_eq!((s1.with_open.n, s1.with_open.popular), (4, 2));
        assert_eq!(s1.with_open.rate_pct, Some(50.0));
        assert_eq!(s1.without_open.n, 0);
        assert_eq!(r.strata[2].with_open.n + r.strata[2].without_open.n, 0);
    }

    #[test]
    fn reach_tag_gaps_only_for_big_bands() {
        // 「なし」帯 30 件: 人気 10 件が全部「休暇あり」、全体では 20 件が「休暇あり」
        let mut v = many("人気,休暇あり", 10);
        v.extend(many("休暇あり", 10));
        v.extend(many("昇給あり", 10));
        // 「1つ」帯は 29 件で出さない
        v.extend(many("学歴不問", 29));
        let r = compute(&v).unwrap().reach;
        let b0 = &r.band_tags[0];
        assert!(b0.shown);
        assert_eq!((b0.n, b0.popular), (30, 10));
        let k = b0.more.iter().find(|g| g.tag == "休暇あり").unwrap();
        assert_eq!(k.n, 20);
        // 100% − 20/30 = 33.33
        assert!((k.diff - (100.0 - 100.0 * 20.0 / 30.0)).abs() < 1e-9);
        let u = b0.less.iter().find(|g| g.tag == "昇給あり").unwrap();
        assert_eq!(u.n, 10);
        assert!((u.diff + 100.0 / 3.0).abs() < 1e-9);
        let b1 = &r.band_tags[1];
        assert!(!b1.shown);
        assert_eq!(b1.n, 29);
        assert!(b1.more.is_empty() && b1.less.is_empty());
    }

    #[test]
    fn reach_tag_gap_needs_three_and_skips_open_words() {
        let mut v = many("人気,休暇あり,寮完備", 2);
        v.extend(many("昇給あり", 28));
        let r = compute(&v).unwrap().reach;
        let b0 = &r.band_tags[0];
        assert!(b0.shown);
        // 休暇あり・寮完備は 2 件なので出ない
        assert!(b0.more.is_empty());
        assert_eq!(b0.less.len(), 1);
        assert_eq!(b0.less[0].tag, "昇給あり");
    }

    #[test]
    fn reach_json_has_no_titles() {
        let json =
            serde_json::to_string(&compute(&many("人気,学歴不問", 40)).unwrap().reach).unwrap();
        assert!(!json.contains("秘密のタイトル"));
    }

    /// 実 CSV で試算値が再現するか (cargo test --lib real_csv_reach -- --ignored --nocapture)。
    #[test]
    #[ignore]
    fn real_csv_reach() {
        let path = "/Users/s_fujimaki/Downloads/indeed-2026-10-26.csv";
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let recs = parse_csv_bytes_with_hints(&bytes, None, UserSourceHint::Auto).unwrap();
        let r = compute(&recs).unwrap().reach;
        eprintln!("{r:#?}");
        let b: Vec<_> = r.bands.iter().map(|x| (x.n, x.popular)).collect();
        assert_eq!(b[0].0, 203);
        assert_eq!(b[1].0, 42);
        assert_eq!(b[2].0, 77);
        let rate = |i: usize| r.bands[i].rate_pct.unwrap().round() as i64;
        assert_eq!((rate(0), rate(1), rate(2)), (25, 31, 55));
        let st = |i: usize| {
            (
                r.strata[i].without_open.rate_pct.map(|x| x.round() as i64),
                r.strata[i].with_open.rate_pct.map(|x| x.round() as i64),
            )
        };
        assert_eq!(st(1), (Some(27), Some(58)));
        assert_eq!(st(2), (Some(26), Some(45)));
    }
}
