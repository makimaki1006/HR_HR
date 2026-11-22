//! HRハッカーに掲載中の求人の、給与の入力の誤りを見つける判定（純粋な関数）。
//!
//! 入力は HubSpot の求人票 (0-420) の `hrh_kyuujinhyou_honbun`。HRハッカーの CSV と同じ
//! 「列名：値」の並び（値は次の列名の行まで）で、列名は [`HRHACKER_COLUMNS`]。
//!
//! 見るもの:
//! - 種類と額の桁（時給・日給・月給・年俸の額がその種類の幅に入るか）
//! - 入力した額と、給与補足の文に書いた額の食い違い
//! - 想定年収と月給の桁
//! - 都道府県の最低賃金を下回っていないか
//!
//! HubSpot の読み取りと API は `handlers::job_copy_live::salary_check`。

use crate::job_gen::hrhacker::HRHACKER_COLUMNS;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// 最低賃金の出典。画面にもこの名前で出す。
pub const MIN_WAGE_SOURCE: &str = "最賃割れチェックシート「最賃リスト」の 2026年最低賃金(円)";

/// 都道府県ごとの最低賃金（円/時）。
/// 出典: 社内のシート「最賃割れチェックシート」の「最賃リスト」タブ、見出し「2026年最低賃金(円)」（2026-11-21 に写した）。
/// `data/minimum_wage_rates.csv` の 2026 年度の行と 47 件とも同じ額。あちらは発効日で選ぶ表で、
/// 発効日が先の県は前年度の額を返すため、掲載の点検ではシートと同じ 2026 年度の額をそのまま使う。
const MIN_WAGE_2026: [(&str, i64); 47] = [
    ("東京都", 1280),
    ("神奈川県", 1279),
    ("大阪府", 1231),
    ("埼玉県", 1196),
    ("千葉県", 1195),
    ("愛知県", 1195),
    ("京都府", 1180),
    ("兵庫県", 1172),
    ("静岡県", 1154),
    ("三重県", 1143),
    ("広島県", 1141),
    ("茨城県", 1136),
    ("滋賀県", 1136),
    ("北海道", 1131),
    ("栃木県", 1125),
    ("岐阜県", 1121),
    ("群馬県", 1120),
    ("富山県", 1119),
    ("長野県", 1117),
    ("福岡県", 1114),
    ("石川県", 1113),
    ("山梨県", 1113),
    ("福井県", 1112),
    ("新潟県", 1108),
    ("奈良県", 1107),
    ("岡山県", 1104),
    ("徳島県", 1103),
    ("和歌山県", 1101),
    ("山口県", 1101),
    ("宮城県", 1098),
    ("大分県", 1096),
    ("佐賀県", 1095),
    ("福島県", 1094),
    ("愛媛県", 1093),
    ("山形県", 1092),
    ("島根県", 1092),
    ("香川県", 1092),
    ("熊本県", 1092),
    ("青森県", 1090),
    ("岩手県", 1090),
    ("秋田県", 1090),
    ("鳥取県", 1090),
    ("鹿児島県", 1090),
    ("長崎県", 1087),
    ("高知県", 1086),
    ("沖縄県", 1086),
    ("宮崎県", 1085),
];

pub fn min_wage(prefecture: &str) -> Option<i64> {
    MIN_WAGE_2026
        .iter()
        .find(|(p, _)| *p == prefecture)
        .map(|(_, v)| *v)
}

/// 指摘の区分。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    /// 都道府県の最低賃金を下回る。
    BelowMinWage,
    /// 給与の種類（時給・月給など）と額の桁が合わない。
    TypeBand,
    /// 入力した額と本文の額が 10 倍・100 倍などで違う。
    TextDigits,
    /// 想定年収と月給の桁が違う。
    AnnualDigits,
    /// 入力した額と本文の額が違う。
    TextAmount,
    /// 入力した額に固定残業代を足すと本文の額になる。
    FixedOtExcluded,
    /// 本給の欄に試用期間の額が入っている。
    TrialAmount,
    // ---- 記載の誤り（`crate::listing_text_check`）----
    /// 「確認中」「要確認」など、書きかけの語が残っている。
    Placeholder,
    /// 日本語の文字の間に「?」などがあり、文字化けしている。
    Mojibake,
    /// 金額の書き方の誤り（1 億円以上・カンマの位置・単位の抜け）。
    AmountTypo,
    /// 必要な欄が空。
    EmptyFields,
    /// 案件名・キャッチコピーに書いた額が、入力した給与と合わない。
    HeadlineAmount,
    /// 本文の中で年間休日の日数が食い違う。
    HolidayMismatch,
    /// 働く時間が分からず、最低賃金と比べられない。
    UnknownHours,
    /// 都道府県が分からず、最低賃金と比べられない。
    UnknownPrefecture,
}

/// 指摘の強さ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Strong,
    Medium,
    Weak,
    /// 記載の誤り。
    Text,
    Unknown,
}

impl Category {
    pub const ALL: [Category; 15] = [
        Category::BelowMinWage,
        Category::TypeBand,
        Category::TextDigits,
        Category::AnnualDigits,
        Category::TextAmount,
        Category::FixedOtExcluded,
        Category::TrialAmount,
        Category::Placeholder,
        Category::Mojibake,
        Category::AmountTypo,
        Category::EmptyFields,
        Category::HeadlineAmount,
        Category::HolidayMismatch,
        Category::UnknownHours,
        Category::UnknownPrefecture,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Category::BelowMinWage => "below_min_wage",
            Category::TypeBand => "type_band",
            Category::TextDigits => "text_digits",
            Category::AnnualDigits => "annual_digits",
            Category::TextAmount => "text_amount",
            Category::FixedOtExcluded => "fixed_overtime_excluded",
            Category::TrialAmount => "trial_amount",
            Category::Placeholder => "placeholder",
            Category::Mojibake => "mojibake",
            Category::AmountTypo => "amount_typo",
            Category::EmptyFields => "empty_fields",
            Category::HeadlineAmount => "headline_amount",
            Category::HolidayMismatch => "holiday_mismatch",
            Category::UnknownHours => "unknown_hours",
            Category::UnknownPrefecture => "unknown_prefecture",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Category::BelowMinWage => "最低賃金を下回る",
            Category::TypeBand => "種類と額の桁が合わない",
            Category::TextDigits => "本文と桁が違う",
            Category::AnnualDigits => "想定年収と桁が違う",
            Category::TextAmount => "本文と額が違う",
            Category::FixedOtExcluded => "本給に固定残業代が入っていない",
            Category::TrialAmount => "試用期間の額が入っている",
            Category::Placeholder => "書きかけの語",
            Category::Mojibake => "文字化け",
            Category::AmountTypo => "金額の書き間違い",
            Category::EmptyFields => "必要な欄が空",
            Category::HeadlineAmount => "題名・キャッチコピーの額",
            Category::HolidayMismatch => "年間休日の食い違い",
            Category::UnknownHours => "判断できない（時間）",
            Category::UnknownPrefecture => "判断できない（都道府県）",
        }
    }
    pub fn severity(self) -> Severity {
        match self {
            Category::BelowMinWage
            | Category::TypeBand
            | Category::TextDigits
            | Category::AnnualDigits => Severity::Strong,
            Category::TextAmount | Category::FixedOtExcluded => Severity::Medium,
            Category::TrialAmount => Severity::Weak,
            Category::Placeholder
            | Category::Mojibake
            | Category::AmountTypo
            | Category::EmptyFields
            | Category::HeadlineAmount
            | Category::HolidayMismatch => Severity::Text,
            Category::UnknownHours | Category::UnknownPrefecture => Severity::Unknown,
        }
    }
}

impl Severity {
    pub fn key(self) -> &'static str {
        match self {
            Severity::Strong => "strong",
            Severity::Medium => "medium",
            Severity::Weak => "weak",
            Severity::Text => "text",
            Severity::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub category: Category,
    pub detail: String,
}

pub(crate) fn finding(category: Category, detail: String) -> Finding {
    Finding { category, detail }
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("固定の正規表現"))
}

/// 全角の数字と記号を半角にそろえる。
pub(crate) fn half(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '０'..='９' => char::from(b'0' + (c as u32 - '０' as u32) as u8),
            '，' => ',',
            '．' => '.',
            '：' => ':',
            '〜' | '～' => '~',
            other => other,
        })
        .collect()
}

/// 本文を「列名 → 値」に分ける。同じ列名が 2 度出たら後のものを使う。
pub fn parse_body(body: &str) -> HashMap<&'static str, String> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    let pat = PAT.get_or_init(|| {
        let mut keys: Vec<&str> = HRHACKER_COLUMNS.to_vec();
        keys.sort_by_key(|k| std::cmp::Reverse(k.chars().count()));
        keys.dedup();
        let alt: Vec<String> = keys.iter().map(|k| regex::escape(k)).collect();
        Regex::new(&format!("(?m)^({})：", alt.join("|"))).expect("列名の正規表現")
    });
    let marks: Vec<(usize, usize, &str)> = pat
        .captures_iter(body)
        .filter_map(|c| {
            let whole = c.get(0)?;
            Some((whole.start(), whole.end(), c.get(1)?.as_str()))
        })
        .collect();
    let mut out = HashMap::new();
    for (i, (_, end, key)) in marks.iter().enumerate() {
        let until = marks.get(i + 1).map_or(body.len(), |next| next.0);
        if let Some(col) = HRHACKER_COLUMNS.iter().find(|c| *c == key) {
            out.insert(*col, body[*end..until].trim().to_string());
        }
    }
    out
}

/// 先頭の数（小数は切り捨て）。数で始まらなければ無し。
fn to_int(s: Option<&str>) -> Option<i64> {
    let s = half(s?).replace(',', "");
    let s = s.trim();
    let int_len = s.bytes().take_while(u8::is_ascii_digit).count();
    if int_len == 0 {
        return None;
    }
    let rest = &s[int_len..];
    let frac_len = rest
        .strip_prefix('.')
        .map(|r| r.bytes().take_while(u8::is_ascii_digit).count())
        .filter(|n| *n > 0)
        .map_or(0, |n| n + 1);
    s[..int_len + frac_len]
        .parse::<f64>()
        .ok()
        .map(|v| v as i64)
}

/// 0 と未入力を「無い」として扱う。
fn nz(v: Option<i64>) -> Option<i64> {
    v.filter(|v| *v != 0)
}

/// 文中の金額（円・万円）をすべて。
fn amounts(text: &str) -> Vec<i64> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    let pat = re(
        &PAT,
        r"([0-9][0-9,]*(?:\.[0-9]+)?)\s*(万)([0-9][0-9,]*)?\s*円|([0-9]{1,3}(?:,[0-9]{3})+|[0-9]{3,})\s*円",
    );
    let t = half(text);
    let mut out = Vec::new();
    for c in pat.captures_iter(&t) {
        if c.get(2).is_some() {
            let man = c[1].replace(',', "").parse::<f64>().unwrap_or(0.0);
            let rest = c
                .get(3)
                .and_then(|m| m.as_str().replace(',', "").parse::<f64>().ok())
                .unwrap_or(0.0);
            out.push((man * 10000.0 + rest) as i64);
        } else if let Some(v) = c
            .get(4)
            .and_then(|m| m.as_str().replace(',', "").parse::<i64>().ok())
        {
            out.push(v);
        }
    }
    out
}

/// 行頭の「月給：」「時給」などと、その額（範囲なら 2 つ）。
fn labeled(text: &str) -> Vec<(String, Vec<i64>)> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    const AMOUNT: &str = r"[0-9][0-9,]*(?:\.[0-9]+)?\s*万?\s*[0-9]*[0-9,]*\s*円";
    let pat = PAT.get_or_init(|| {
        Regex::new(&format!(
            r"(?m)^[ \t　]*[・●■◆]?[ \t　]*(月給|時給|日給|年俸)[ \t　]*[:：]?[ \t　]*(?:[^0-9\n例]{{0,4}}?)({AMOUNT})(?:\s*[~\-]\s*({AMOUNT}))?"
        ))
        .expect("給与の見出しの正規表現")
    });
    let t = half(text);
    pat.captures_iter(&t)
        .map(|c| {
            let mut vs = amounts(&c[2]);
            if let Some(second) = c.get(3) {
                vs.extend(amounts(second.as_str()));
            }
            (c[1].to_string(), vs)
        })
        .collect()
}

/// 年収例・月収例などの区画と、例・手当・深夜・UP などの行を外す。
fn strip_examples(text: &str) -> String {
    static HEAD: OnceLock<Regex> = OnceLock::new();
    static HEAD_SKIP: OnceLock<Regex> = OnceLock::new();
    static LINE_SKIP: OnceLock<Regex> = OnceLock::new();
    let head = re(&HEAD, r"^\s*[【＜<■◆●]");
    let head_skip = re(&HEAD_SKIP, "例|手当|賞与|昇給");
    let line_skip = re(&LINE_SKIP, "例|手当|深夜|UP|アップ|賞与|昇給|場合|＝|=");
    let mut out = Vec::new();
    let mut skip = false;
    for line in text.split('\n') {
        if head.is_match(line) {
            skip = head_skip.is_match(line);
        }
        if skip || line_skip.is_match(line) {
            continue;
        }
        out.push(line);
    }
    out.join("\n")
}

/// 給与の種類ごとの、額の幅（円）。
const BAND: [(&str, i64, i64); 4] = [
    ("時給", 800, 5_000),
    ("日給", 3_000, 50_000),
    ("月給", 100_000, 1_500_000),
    ("年俸", 1_500_000, 20_000_000),
];

fn band_of(ty: &str) -> Option<(i64, i64)> {
    BAND.iter()
        .find(|(t, _, _)| *t == ty)
        .map(|(_, lo, hi)| (*lo, *hi))
}

/// 週 40 時間 × 52 週 ÷ 12 か月。
const MONTH_MAX_HOURS: f64 = 40.0 * 52.0 / 12.0;

fn plausible_hours(h: Option<i64>) -> Option<i64> {
    h.filter(|h| (3..=24).contains(h))
}

/// 勤務時間の文から 1 日の時間。「実働○時間」か、始業〜終業から休憩を引いたもの。
fn hours_from_text(text: &str) -> Option<f64> {
    static ACTUAL: OnceLock<Regex> = OnceLock::new();
    static RANGE: OnceLock<Regex> = OnceLock::new();
    static BREAK: OnceLock<Regex> = OnceLock::new();
    let t = half(text);
    if let Some(c) = re(
        &ACTUAL,
        r"実働\s*(?:時間)?\s*[::]?\s*([0-9]+(?:\.[0-9]+)?)\s*時間",
    )
    .captures(&t)
    {
        return c[1].parse::<f64>().ok();
    }
    let c = re(
        &RANGE,
        r"([0-9]{1,2}):([0-9]{2})\s*[~\-]\s*([0-9]{1,2}):([0-9]{2})",
    )
    .captures(&t)?;
    if t.contains("シフト") {
        return None;
    }
    let n = |i: usize| c[i].parse::<i64>().unwrap_or(0);
    let start = n(1) * 60 + n(2);
    let mut end = n(3) * 60 + n(4);
    if end <= start {
        end += 1440;
    }
    let span = end - start;
    let rest = re(&BREAK, r"休憩\s*(?:時間)?\s*[::]?\s*([0-9]+)\s*分")
        .captures(&t)
        .and_then(|c| c[1].parse::<i64>().ok())
        .unwrap_or(if span > 480 {
            60
        } else if span > 360 {
            45
        } else {
            0
        });
    let h = (span - rest) as f64 / 60.0;
    (3.0..=13.0).contains(&h).then_some(h)
}

fn fixed_overtime(v: Option<&str>) -> i64 {
    let Some(v) = v.filter(|v| !v.is_empty()) else {
        return 0;
    };
    let s = half(v);
    let s = s.trim();
    if s.starts_with("なし") || s == "0" {
        return 0;
    }
    amounts(s.split('\n').next().unwrap_or(""))
        .first()
        .copied()
        .or_else(|| nz(to_int(Some(s))))
        .unwrap_or(0)
}

fn annual_holidays(body: &str) -> Option<i64> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    re(&PAT, r"年間休日\s*[::]?\s*([0-9]{2,3})\s*日")
        .captures(&half(body))
        .and_then(|c| c[1].parse().ok())
}

/// 3 桁ごとのカンマ。
pub(crate) fn yen(v: i64) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if v < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// 小数を四捨五入してカンマ付きにする。
fn yen_f(v: f64) -> String {
    yen(format!("{v:.0}").parse::<i64>().unwrap_or(0))
}

/// 有効 6 桁で、末尾の 0 を落とす（160 / 7.5 / 163.333）。
fn short_number(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let exp = v.abs().log10().floor() as i32;
    let decimals = (5 - exp).clamp(0, 12) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

struct Pay {
    /// 「本給」か「試用期間」。
    kind: &'static str,
    ty: String,
    lo: Option<i64>,
    hi: Option<i64>,
    hours: Option<i64>,
    days: Option<i64>,
    fixed: i64,
    /// 入力した額に固定残業代が入っていないと分かった（最低賃金の比較で引かない）。
    fixed_excluded: bool,
}

/// 1 件の求人の指摘。`prefecture` は HubSpot の `todoufuken`、`body` は `hrh_kyuujinhyou_honbun`。
pub fn check(prefecture: Option<&str>, body: &str) -> Vec<Finding> {
    let f = parse_body(body);
    let get = |k: &str| f.get(k).map(String::as_str).filter(|v| !v.is_empty());
    let mut res = Vec::new();
    let pref = prefecture.unwrap_or("");
    let mw = min_wage(pref);
    let work_text = get("勤務時間").unwrap_or("");

    let mut pays: Vec<Pay> = Vec::new();
    if let Some(ty) = get("給与形態") {
        pays.push(Pay {
            kind: "本給",
            ty: ty.to_string(),
            lo: nz(to_int(get("基本給与 最小"))),
            hi: nz(to_int(get("基本給与 最大"))),
            hours: to_int(get("平均稼働時間")),
            days: to_int(get("平均稼働日数")),
            fixed: fixed_overtime(get("固定残業代")),
            fixed_excluded: false,
        });
    }
    let trial_type = match get("試用・研修期の給与のタイプ") {
        Some("1") => Some("時給"),
        Some("2") => Some("日給"),
        Some("3") => Some("月給"),
        _ => None,
    };
    let trial_lo = nz(to_int(get("試用・研修期の基本給与 最小")));
    if let (Some(ty), Some(_)) = (trial_type, trial_lo) {
        pays.push(Pay {
            kind: "試用期間",
            ty: ty.to_string(),
            lo: trial_lo,
            hi: nz(to_int(get("試用・研修期の基本給与 最大"))),
            hours: to_int(get("試用・研修期の平均稼働時間")),
            days: to_int(get("試用・研修期の平均稼働日数")),
            fixed: fixed_overtime(get("試用・研修期の固定残業代")),
            fixed_excluded: false,
        });
    }

    // 種類と額の桁
    let mut bad_type: HashSet<&'static str> = HashSet::new();
    for p in &pays {
        let ty = if p.ty == "日給月給制" || p.ty == "月給+歩合" {
            "月給"
        } else {
            p.ty.as_str()
        };
        let Some((lo, hi)) = band_of(ty) else {
            continue;
        };
        for (which, v) in [("最小", p.lo), ("最大", p.hi)] {
            let Some(v) = v else { continue };
            if (lo..=hi).contains(&v) {
                continue;
            }
            let fit: Vec<&str> = BAND
                .iter()
                .filter(|(_, lo, hi)| (*lo..=*hi).contains(&v))
                .map(|(t, _, _)| *t)
                .collect();
            let hint = if fit.is_empty() {
                String::new()
            } else {
                format!("（{}の幅の額）", fit.join("・"))
            };
            res.push(finding(
                Category::TypeBand,
                format!("{}：{} の{}が {} 円{}", p.kind, p.ty, which, yen(v), hint),
            ));
            bad_type.insert(p.kind);
        }
    }

    // 入力した額と本文の額
    static TRIAL_WORD: OnceLock<Regex> = OnceLock::new();
    let sup = get("給与補足").unwrap_or("");
    let cut = re(&TRIAL_WORD, "試用|研修").find(sup).map(|m| m.start());
    let main_text = strip_examples(cut.map_or(sup, |i| &sup[..i]));
    let trial_text = format!(
        "{}\n{}",
        cut.map_or("", |i| &sup[i..]),
        get("試用・研修の詳細情報").unwrap_or("")
    );
    let trial_amounts: HashSet<i64> = amounts(&trial_text).into_iter().collect();
    let trial_inputs: HashSet<i64> = pays
        .iter()
        .filter(|p| p.kind == "試用期間")
        .flat_map(|p| [p.lo, p.hi])
        .flatten()
        .collect();
    if let Some(p) = pays.first_mut().filter(|p| p.kind == "本給") {
        let head = labeled(&main_text)
            .into_iter()
            .find(|(label, vs)| *label == p.ty && !vs.is_empty());
        if let Some((_, text)) = head {
            let shown = text
                .iter()
                .take(2)
                .map(|a| yen(*a))
                .collect::<Vec<_>>()
                .join("〜");
            let ty = p.ty.clone();
            for (which, v, tv) in [
                ("最小", p.lo, text.first().copied()),
                ("最大", p.hi, text.get(1).copied()),
            ] {
                let (Some(v), Some(tv)) = (v, nz(tv)) else {
                    continue;
                };
                if v == tv || (p.fixed != 0 && v - p.fixed == tv) {
                    continue;
                }
                if p.fixed != 0 && v + p.fixed == tv {
                    p.fixed_excluded = true;
                    res.push(finding(
                        Category::FixedOtExcluded,
                        format!(
                            "本給の{which} {} 円＋固定残業代 {} 円＝本文の{ty} {shown} 円",
                            yen(v),
                            yen(p.fixed)
                        ),
                    ));
                    continue;
                }
                if trial_amounts.contains(&v) || trial_inputs.contains(&v) {
                    res.push(finding(
                        Category::TrialAmount,
                        format!(
                            "本給の{which} {} 円は試用期間の額（本文の{ty} {shown} 円）",
                            yen(v)
                        ),
                    ));
                    continue;
                }
                let digits_off = [10_i64, 100, 1_000, 10_000]
                    .iter()
                    .any(|k| v.checked_mul(*k) == Some(tv) || tv.checked_mul(*k) == Some(v));
                res.push(finding(
                    if digits_off {
                        Category::TextDigits
                    } else {
                        Category::TextAmount
                    },
                    format!("本給の{which} {} 円、本文の{ty} {shown} 円", yen(v)),
                ));
            }
        }
    }

    // 想定年収
    static ANNUAL: OnceLock<Regex> = OnceLock::new();
    if let Some(p) = pays.first() {
        let sup_half = half(sup);
        let m = re(
            &ANNUAL,
            r"想定年収\s*[::]?\s*([0-9][0-9,]*(?:\.[0-9]+)?)\s*(万)?",
        )
        .captures(&sup_half);
        if let (Some(c), Some(lo), true) = (m, p.lo, p.ty == "月給" || p.ty == "年俸") {
            let annual = c[1].replace(',', "").parse::<f64>().unwrap_or(0.0)
                * if c.get(2).is_some() { 10000.0 } else { 1.0 };
            let monthly = p.ty == "月給";
            let base = if monthly { lo * 12 } else { lo };
            let ratio = annual / base as f64;
            if ratio >= 5.0 || (ratio > 0.0 && ratio <= 0.2) {
                let basis = if monthly {
                    format!("（×12 で {} 円）", yen(base))
                } else {
                    String::new()
                };
                res.push(finding(
                    Category::AnnualDigits,
                    format!(
                        "想定年収 {} 円、{} {} 円{basis}",
                        yen(annual as i64),
                        p.ty,
                        yen(lo)
                    ),
                ));
            }
        }
    }

    // 最低賃金
    for p in &pays {
        if bad_type.contains(p.kind) {
            continue;
        }
        let Some(lo) = p.lo else { continue };
        let Some(mw) = mw else {
            res.push(finding(
                Category::UnknownPrefecture,
                format!("{}：都道府県が分からない", p.kind),
            ));
            continue;
        };
        let ty = p.ty.as_str();
        if ty == "完全歩合制" {
            continue;
        }
        let (hourly, how) = if ty == "時給" {
            (lo as f64, "時給".to_string())
        } else if ty == "日給" {
            let h = plausible_hours(p.hours)
                .map(|h| h as f64)
                .or_else(|| hours_from_text(work_text))
                .filter(|h| *h != 0.0);
            match h {
                Some(h) => (lo as f64 / h, format!("日給÷{}時間", short_number(h))),
                None => {
                    let per_hour = lo as f64 / 8.0;
                    if per_hour < mw as f64 {
                        res.push(finding(
                            Category::BelowMinWage,
                            format!(
                                "{}：日給 {} 円は 8 時間で割っても {} 円（{pref} {} 円）",
                                p.kind,
                                yen(lo),
                                yen_f(per_hour),
                                yen(mw)
                            ),
                        ));
                    } else {
                        res.push(finding(
                            Category::UnknownHours,
                            format!("{}：日給、1日の時間が分からない", p.kind),
                        ));
                    }
                    continue;
                }
            }
        } else {
            let annual = ty == "年俸";
            let fixed = if p.fixed_excluded { 0 } else { p.fixed };
            let monthly = if annual { lo as f64 / 12.0 } else { lo as f64 } - fixed as f64;
            let minus = if fixed != 0 { "−固定残業代" } else { "" };
            let (h, d) = (nz(p.hours), nz(p.days));
            let mut note = String::new();
            let month_hours = match (h, d) {
                (Some(h), Some(d)) if annual && d > 31 && plausible_hours(Some(h)).is_some() => {
                    Some((h * d) as f64 / 12.0)
                }
                (Some(h), Some(d)) if h <= 24 && d <= 31 && (60..=260).contains(&(h * d)) => {
                    Some((h * d) as f64)
                }
                _ => {
                    let day = hours_from_text(work_text).filter(|h| *h != 0.0);
                    let holidays = annual_holidays(body).filter(|n| (52..=200).contains(n));
                    day.zip(holidays).map(|(day, holidays)| {
                        note = format!(
                            "（勤務時間の文 {} 時間×年間休日 {holidays} 日から）",
                            short_number(day)
                        );
                        day * (365 - holidays) as f64 / 12.0
                    })
                }
            }
            .filter(|mh| *mh != 0.0);
            match month_hours {
                Some(mh) => (
                    monthly / mh,
                    format!(
                        "{note}{}{minus}÷{}時間",
                        if annual { "年俸÷12" } else { "月給" },
                        short_number(mh)
                    ),
                ),
                None => {
                    let per_hour = monthly / MONTH_MAX_HOURS;
                    if per_hour < mw as f64 {
                        res.push(finding(
                            Category::BelowMinWage,
                            format!(
                                "{}：{ty}{minus} {} 円は月 173.3 時間で割っても {} 円（{pref} {} 円）",
                                p.kind,
                                yen_f(monthly),
                                yen_f(per_hour),
                                yen(mw)
                            ),
                        ));
                    } else {
                        let entered = match (p.hours, p.days) {
                            (None, None) => "時間・日数が入っていない".to_string(),
                            (h, d) => format!(
                                "時間・日数が {}時間×{}日",
                                h.map_or("未入力".into(), |h| h.to_string()),
                                d.map_or("未入力".into(), |d| d.to_string())
                            ),
                        };
                        res.push(finding(
                            Category::UnknownHours,
                            format!("{}：{ty}、{entered}", p.kind),
                        ));
                    }
                    continue;
                }
            }
        };
        if hourly < mw as f64 {
            res.push(finding(
                Category::BelowMinWage,
                format!(
                    "{}：{how} = {} 円（{pref} {} 円）",
                    p.kind,
                    yen_f(hourly),
                    yen(mw)
                ),
            ));
        }
    }
    res
}
