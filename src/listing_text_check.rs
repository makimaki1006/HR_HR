//! 求人の記載の誤りを見つける判定（純粋な関数）。入力は HRハッカーの求人本文
//! （HubSpot の `hrh_kyuujinhyou_honbun`、「列名：値」の並び）。
//!
//! 見るもの（2026-11-22、HRハッカー掲載中 1,842 件で試して決めた決まり）:
//! - 書きかけの語: 「確認中」「要確認」「TBD」「TODO」「ダミー」「（仮）」
//! - 文字化け: 日本語の文字の間に半角の「?」か「�」（全角の「？」は除く）
//! - 金額の書き間違い: 給与に関わる欄の 1 億円以上の額、3 桁ごとでないカンマ、3 桁以下の月給・日給
//! - 必要な欄が空: 案件名・仕事内容・応募資格・給与形態・基本給与 最小・給与補足・勤務時間・キャッチコピー
//! - 題名・キャッチコピーの額: 入力した給与の範囲から外れる（書いた桁の分の丸めと「以上」は許す）
//! - 年間休日の食い違い: 本文の中に違う「年間休日○日」が 2 つ以上
//!
//! 「テスト」「未定」「○○」は正しい使い方が多いため見ない。漢字の抜け落ちは機械的に見つけられないため見ない。
//! 1 つの求人・1 つの区分につき指摘は 1 つ（見つかった箇所を説明文に並べる）。

use crate::listing_salary_check::{finding, half, parse_body, yen, Category, Finding};
use regex::Regex;
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("固定の正規表現"))
}

/// `[start, end)` の前 `before` 文字・後 `after` 文字を含む部分。
fn window(s: &str, start: usize, end: usize, before: usize, after: usize) -> &str {
    let from = s[..start]
        .char_indices()
        .rev()
        .nth(before.saturating_sub(1))
        .map_or(0, |(i, _)| i);
    let from = if before == 0 { start } else { from };
    let to = s[end..]
        .char_indices()
        .nth(after)
        .map_or(s.len(), |(i, _)| end + i);
    &s[from..to]
}

/// 改行を空白にして、説明文に載せる短い抜き出しにする。
fn snippet(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 位置 `pos` を含む列の名前（本文の「列名：」の行で区切る）。
fn column_at(fields: &HashMap<&'static str, String>, body: &str, pos: usize) -> &'static str {
    let mut best: Option<(usize, &'static str)> = None;
    for key in fields.keys() {
        let mark = format!("{key}：");
        for (i, _) in body.match_indices(&mark) {
            let at_line_start = i == 0 || body[..i].ends_with('\n');
            if at_line_start && i <= pos && best.map_or(true, |(b, _)| i > b) {
                best = Some((i, *key));
            }
        }
    }
    best.map_or("本文", |(_, k)| k)
}

const REQUIRED: [&str; 8] = [
    "案件名",
    "仕事内容",
    "応募資格",
    "給与形態",
    "基本給与 最小",
    "給与補足",
    "勤務時間",
    "キャッチコピー",
];
const SALARY_COLUMNS: [&str; 5] = [
    "給与補足",
    "試用・研修の詳細情報",
    "案件名",
    "キャッチコピー",
    "メリット",
];
const HEADLINE_COLUMNS: [&str; 3] = ["案件名", "キャッチコピー", "メリット"];
/// 説明文に並べる箇所の上限。
const MAX_PLACES: usize = 3;

fn places(mut items: Vec<String>) -> String {
    let mut seen = BTreeSet::new();
    items.retain(|i| seen.insert(i.clone()));
    let more = items.len().saturating_sub(MAX_PLACES);
    items.truncate(MAX_PLACES);
    let mut out = items.join("／");
    if more > 0 {
        out.push_str(&format!("（ほか {more} か所）"));
    }
    out
}

fn parse_f64(v: Option<&String>) -> Option<f64> {
    match v.map(|s| s.trim()) {
        None | Some("") => Some(0.0),
        Some(s) => s.parse::<f64>().ok(),
    }
}

/// 1 件の求人の記載の誤り。
pub fn check(body: &str) -> Vec<Finding> {
    let fields = parse_body(body);
    let get = |k: &str| fields.get(k).map(String::as_str).unwrap_or("");
    let mut res = Vec::new();

    // 書きかけの語
    static PLACEHOLDER: OnceLock<Regex> = OnceLock::new();
    let found: Vec<String> = re(&PLACEHOLDER, r"確認中|要確認|TBD|TODO|ダミー|（仮）|\(仮\)")
        .find_iter(body)
        .map(|m| {
            format!(
                "{}「{}」",
                column_at(&fields, body, m.start()),
                snippet(window(body, m.start(), m.end(), 8, 4))
            )
        })
        .collect();
    if !found.is_empty() {
        res.push(finding(Category::Placeholder, places(found)));
    }

    // 文字化け
    static MOJIBAKE: OnceLock<Regex> = OnceLock::new();
    let hits: Vec<_> = re(
        &MOJIBAKE,
        "[ぁ-んァ-ヶ一-龥ー][?\u{FFFD}][ぁ-んァ-ヶ一-龥ー]|\u{FFFD}",
    )
    .find_iter(body)
    .collect();
    if let Some(first) = hits.first() {
        let example = snippet(window(body, first.start(), first.end(), 6, 6));
        let detail = if hits.len() > 1 {
            format!("「{example}」など {} か所", hits.len())
        } else {
            format!("{}「{example}」", column_at(&fields, body, first.start()))
        };
        res.push(finding(Category::Mojibake, detail));
    }

    // 金額の書き間違い
    let mut typos = Vec::new();
    let salary_text = half(
        &SALARY_COLUMNS
            .iter()
            .map(|k| get(k))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    static MAN: OnceLock<Regex> = OnceLock::new();
    for c in re(&MAN, r"([0-9][0-9,]*(?:\.[0-9]+)?)\s*万\s*円").captures_iter(&salary_text) {
        if c[1].replace(',', "").parse::<f64>().unwrap_or(0.0) >= 10000.0 {
            typos.push(format!("「{}」（1 億円以上）", &c[0]));
        }
    }
    static COMMA: OnceLock<Regex> = OnceLock::new();
    let body_half = half(body);
    for c in re(
        &COMMA,
        r"(?:^|[^0-9,.])([0-9]{1,3}(?:,[0-9]+)+)\s*(万)?\s*円",
    )
    .captures_iter(&body_half)
    {
        let num = &c[1];
        if num.split(',').skip(1).any(|p| p.len() != 3) {
            let whole = c.get(1).map_or("", |m| m.as_str());
            let tail = &c[0][c[0].find(whole).unwrap_or(0) + whole.len()..];
            typos.push(format!("「{whole}{tail}」（カンマの位置）"));
        }
    }
    static NO_UNIT: OnceLock<Regex> = OnceLock::new();
    for m in re(&NO_UNIT, r"(月給|日給)\s*[:]?\s*([0-9]{1,3})\s*円").find_iter(&salary_text) {
        typos.push(format!("「{}」（額が小さすぎる）", m.as_str()));
    }
    if !typos.is_empty() {
        res.push(finding(Category::AmountTypo, places(typos)));
    }

    // 必要な欄が空
    let empty: Vec<&str> = REQUIRED
        .iter()
        .copied()
        .filter(|k| get(k).trim().is_empty())
        .collect();
    if !empty.is_empty() {
        let detail = if empty.len() == REQUIRED.len() {
            "本文の欄がすべて空".to_string()
        } else {
            format!("空の欄：{}", empty.join("・"))
        };
        res.push(finding(Category::EmptyFields, detail));
    }

    // 題名・キャッチコピーの額
    let ty = get("給与形態");
    let range = parse_f64(fields.get("基本給与 最小"))
        .zip(parse_f64(fields.get("基本給与 最大")))
        .map(|(lo, hi)| (lo, if hi == 0.0 { lo } else { hi }));
    if let Some((lo, hi)) = range.filter(|(lo, _)| *lo != 0.0) {
        static HEAD: OnceLock<Regex> = OnceLock::new();
        static NEAR: OnceLock<Regex> = OnceLock::new();
        let mut off = Vec::new();
        for col in HEADLINE_COLUMNS {
            let t = half(get(col));
            for c in re(
                &HEAD,
                r"(月給|時給|日給)\s*[:]?\s*([0-9][0-9,]*(?:\.[0-9]+)?)\s*(万)?\s*円",
            )
            .captures_iter(&t)
            {
                let kind = &c[1];
                if kind != ty && !((ty == "日給月給制" || ty == "月給+歩合") && kind == "月給")
                {
                    continue;
                }
                let whole = c.get(0).expect("一致");
                let near = window(&t, whole.start(), whole.end(), 12, 8);
                if re(&NEAR, "例|年目|以上も|可能|目指|昇給|UP|アップ|最大|最高|平均|後は|入社|実績|モデル|保証|早朝|深夜|夜勤|日曜|祝日|土日|休日|手当|込|含|ヶ月|か月|カ月").is_match(near) {
                    continue;
                }
                let num = &c[2];
                let man = c.get(3).is_some();
                let v = num.replace(',', "").parse::<f64>().unwrap_or(0.0)
                    * if man { 10000.0 } else { 1.0 };
                let p = if !man {
                    1.0
                } else if let Some((_, dec)) = num.split_once('.') {
                    10000.0 / 10f64.powi(dec.len() as i32)
                } else {
                    10000.0
                };
                let ijou = t[whole.end()..].starts_with("以上");
                let ok =
                    (lo - p < v && v <= hi + p - 1e-9) || (v - lo).abs() < p || (ijou && v <= lo);
                if !ok {
                    let range = if hi > lo {
                        format!("{}〜{}", yen(lo as i64), yen(hi as i64))
                    } else {
                        yen(lo as i64)
                    };
                    off.push(format!(
                        "{col}の「{}」、入力した{ty}は {range} 円",
                        whole.as_str()
                    ));
                }
            }
        }
        if !off.is_empty() {
            res.push(finding(Category::HeadlineAmount, places(off)));
        }
    }

    // 年間休日の食い違い
    static HOLIDAY: OnceLock<Regex> = OnceLock::new();
    static HOLIDAY_NEAR: OnceLock<Regex> = OnceLock::new();
    let mut days = BTreeSet::new();
    for c in
        re(&HOLIDAY, r"年間休日\s*[:]?\s*(?:約)?\s*([0-9]{2,3})\s*日").captures_iter(&body_half)
    {
        let whole = c.get(0).expect("一致");
        let near = window(&body_half, whole.start(), whole.end(), 6, 6);
        if re(
            &HOLIDAY_NEAR,
            "以上|~|〜|実質|目指|パターン|最大|最低|平均|前後",
        )
        .is_match(near)
        {
            continue;
        }
        // 「当年度土曜日年間休日:37日」は土曜の休みの日数で、年間休日ではない。
        // 後ろ側は「年間休日120日(土日祝日休み)」の「祝日」を拾うので、直前の語だけを見る。
        let just_before = window(&body_half, whole.start(), whole.start(), 3, 0);
        if ["土曜日", "日曜日", "土曜", "日曜", "祝日", "土日"]
            .iter()
            .any(|w| just_before.ends_with(w))
        {
            continue;
        }
        if let Ok(d) = c[1].parse::<u32>() {
            days.insert(d);
        }
    }
    if days.len() > 1 {
        let list: Vec<String> = days.iter().map(|d| format!("{d} 日")).collect();
        res.push(finding(
            Category::HolidayMismatch,
            format!("年間休日が {} と書いてある", list.join("・")),
        ));
    }
    res
}
