//! AirWork に掲載中の求人の、給与と記載の誤りを見つける判定（純粋な関数）。
//!
//! 入力は HubSpot の求人票 (0-420) の `aw_kyuujinhyou_honbun`。AirWork の公開ページの項目を
//! [`AIRWORK_COLUMNS`] の順に「列名：値」でつないだ全文（値は次の列名の行まで。空の項目も「列名：」の行は残る）。
//! 書き込むのは専用PCの定期取得で、HR_HR は読むだけ。
//!
//! 決まりは 2026-11-23 に AirWork 公開中 1,537 件で試して決めたもの。区分は HRハッカーの求人チェックに
//! そろえ（[`Category`] を共有）、AirWork にしか無いもの（固定残業代の食い違い）だけを足した。
//! HRハッカーと違い、給与は「給与」の 1 行（例「月給 22万710円 〜 26万9700円」）と、「給与の詳細」の
//! 「基本給：」「固定残業代：あり／なし」の行から読む。最低賃金は「勤務地」の住所の都道府県で比べ、
//! 時間は「勤務時間」の「実働時間：1日あたり」「平均勤務日数：1ヶ月あたり」「総労働時間：1ヶ月あたり」から求める。
//!
//! 1 つの求人・1 つの区分につき指摘は 1 つ（見つかった箇所を説明文に並べる）。

use crate::listing_salary_check::{finding, min_wage, yen, Category, Finding};
use regex::Regex;
use std::sync::OnceLock;

/// `aw_kyuujinhyou_honbun` の列名と順番。
pub const AIRWORK_COLUMNS: [&str; 12] = [
    "キャッチコピー",
    "給与",
    "給与の詳細",
    "仕事内容",
    "職種",
    "求める人材",
    "勤務時間",
    "休日休暇",
    "社会保険・福利厚生",
    "試用・研修期間",
    "勤務地",
    "アクセス",
];

/// 点検する欄（職種は選択肢なので見ない）。年間休日の食い違いはこの順につないだ文で探す。
const CHECKED: [&str; 11] = [
    "キャッチコピー",
    "給与",
    "給与の詳細",
    "仕事内容",
    "求める人材",
    "勤務時間",
    "休日休暇",
    "社会保険・福利厚生",
    "試用・研修期間",
    "勤務地",
    "アクセス",
];

const REQUIRED: [&str; 7] = [
    "キャッチコピー",
    "給与",
    "仕事内容",
    "求める人材",
    "勤務時間",
    "勤務地",
    "休日休暇",
];

const DRAFT: [&str; 7] = [
    "確認中",
    "要確認",
    "TBD",
    "TODO",
    "ダミー",
    "（仮）",
    "(仮)",
];

/// キャッチコピーの額の近くにこの語があれば、給与と比べない（例・平均・手当など）。
const SKIP_NEAR: [&str; 12] = [
    "例",
    "年目",
    "平均",
    "最高",
    "保証",
    "早朝",
    "日祝",
    "手当",
    "賞与",
    "インセンティブ",
    "歩合",
    "年収",
];

/// 給与の種類ごとの、額の幅（円）。AirWork の「年俸」は「年収」として扱う。
const BAND: [(&str, f64, f64); 4] = [
    ("時給", 800.0, 5_000.0),
    ("日給", 3_000.0, 50_000.0),
    ("月給", 100_000.0, 1_500_000.0),
    ("年収", 1_500_000.0, 20_000_000.0),
];

/// 週 40 時間 × 52 週 ÷ 12 か月。
const MONTH_HOURS: f64 = 40.0 * 52.0 / 12.0;

/// 説明文に並べる箇所の上限。
const MAX_PLACES: usize = 3;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("固定の正規表現"))
}

/// 全角の数字・カンマ・ピリオドを半角に、「～」を「〜」にそろえる（1 文字ずつ置き換える）。
fn z2h(s: &str) -> String {
    convert(s, true)
}

/// [`z2h`] のうち、全角のカンマだけはそのまま残す。
fn z2h_keep_comma(s: &str) -> String {
    convert(s, false)
}

fn convert(s: &str, comma: bool) -> String {
    s.chars()
        .map(|c| match c {
            '０'..='９' => char::from(b'0' + (c as u32 - '０' as u32) as u8),
            '，' if comma => ',',
            '．' => '.',
            '～' => '〜',
            other => other,
        })
        .collect()
}

/// `byte` の位置から後ろ `n` 文字。
fn after(s: &str, byte: usize, n: usize) -> &str {
    let rest = &s[byte..];
    let end = rest.char_indices().nth(n).map_or(rest.len(), |(i, _)| i);
    &rest[..end]
}

/// `byte` の位置より前 `n` 文字。
fn before(s: &str, byte: usize, n: usize) -> &str {
    let head = &s[..byte];
    let start = if n == 0 {
        byte
    } else {
        head.char_indices().rev().nth(n - 1).map_or(0, |(i, _)| i)
    };
    &head[start..]
}

/// 説明文に載せる、`byte` の前後 14 文字。
fn snippet(s: &str, byte: usize) -> String {
    format!("{}{}", before(s, byte, 14), after(s, byte, 14)).replace('\n', " ")
}

/// 行頭にある「列名：」の位置（`from` から後ろ）。
fn marks_of<'a>(body: &'a str, mark: &'a str, from: usize) -> impl Iterator<Item = usize> + 'a {
    body[from..]
        .match_indices(mark)
        .map(move |(i, _)| from + i)
        .filter(move |i| *i == 0 || body[..*i].ends_with('\n'))
}

/// 本文を列ごとに分ける。列は決まった順に並ぶので、前の列の後ろから次の列名の行を探す
/// （勤務時間の中の「勤務時間：9:00〜」、試用・研修期間の中の「試用・研修期間：」のような行で切れないように）。
/// 職種だけは、仕事内容の文の中に「職種：」の行があることが多いため、次の「求める人材：」の行の直前のものを使う。
/// 無い列は空。値の後ろの改行（列をつなぐ改行）は落とす。
pub fn parse_body(body: &str) -> Vec<(&'static str, String)> {
    let mut marks: Vec<(&'static str, usize, usize)> = Vec::new();
    let mut pos = 0;
    for (k, col) in AIRWORK_COLUMNS.iter().enumerate() {
        let mark = format!("{col}：");
        let found = if *col == "職種" {
            let next = AIRWORK_COLUMNS
                .get(k + 1)
                .and_then(|n| marks_of(body, &format!("{n}："), pos).next());
            marks_of(body, &mark, pos)
                .take_while(|i| next.is_none_or(|n| *i < n))
                .last()
        } else {
            marks_of(body, &mark, pos).next()
        };
        if let Some(i) = found {
            marks.push((col, i, i + mark.len()));
            pos = i + mark.len();
        }
    }
    AIRWORK_COLUMNS
        .iter()
        .map(|col| {
            let value = marks
                .iter()
                .position(|(c, _, _)| c == col)
                .map(|k| match marks.get(k + 1) {
                    Some(next) => {
                        let v = &body[marks[k].2..next.1];
                        let v = v.strip_suffix('\n').unwrap_or(v);
                        v.strip_suffix('\r').unwrap_or(v).to_string()
                    }
                    None => body[marks[k].2..].to_string(),
                })
                .unwrap_or_default();
            (*col, value)
        })
        .collect()
}

/// 「22万710円」「1,200円」「26万円」→ 円。
fn yen_jp(s: &str) -> Option<f64> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    let s = z2h(s).replace([',', ' '], "");
    let c = re(&PAT, r"^(?:(\d+(?:\.\d+)?)万)?(\d+)?円?$").captures(&s)?;
    if c.get(1).is_none() && c.get(2).is_none() {
        return None;
    }
    let man = match c.get(1) {
        Some(m) => m.as_str().parse::<f64>().ok()?,
        None => 0.0,
    };
    let rest = match c.get(2) {
        Some(m) => m.as_str().parse::<i64>().ok()? as f64,
        None => 0.0,
    };
    Some((man * 10000.0 + rest).round_ties_even())
}

/// 給与の種類（年俸は年収にそろえる）。
fn kind_of(word: &str) -> &'static str {
    match word {
        "時給" => "時給",
        "日給" => "日給",
        "月給" => "月給",
        _ => "年収",
    }
}

struct Pay {
    kind: &'static str,
    lo: Option<f64>,
    hi: Option<f64>,
}

/// 「月給 22万710円 〜 26万9700円」のような 1 行の給与。
fn parse_structured(s: &str) -> Option<Pay> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    let t = z2h(s);
    let c = re(
        &PAT,
        r"(時給|日給|月給|年収|年俸)\s*([0-9.,万]+円?)\s*(?:〜\s*([0-9.,万]+円?))?",
    )
    .captures(&t)?;
    Some(Pay {
        kind: kind_of(&c[1]),
        lo: yen_jp(&c[2]),
        hi: c.get(3).and_then(|m| yen_jp(m.as_str())),
    })
}

/// 文中の「月給 25万円」などの額。
struct Amount {
    kind: &'static str,
    yen: f64,
    /// 書いた桁の分の丸め（「25万円」なら 1 万円）。
    tol: f64,
    /// 「以上」「〜」が続く。
    at_least: bool,
    around: String,
    text: String,
}

fn free_amounts(text: &str) -> Vec<Amount> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    static RANGE_MAN: OnceLock<Regex> = OnceLock::new();
    let t = z2h(text);
    let pat = re(
        &PAT,
        r"(時給|日給|月給|年収|年俸)\s*[:：]?\s*([0-9][0-9,.]*)\s*(万|千)?\s*([0-9]{1,4})?\s*円?",
    );
    let mut out = Vec::new();
    for c in pat.captures_iter(&t) {
        let whole = c.get(0).expect("一致");
        let raw = c[2].trim_end_matches(['.', ',']);
        let Ok(v) = raw.replace(',', "").parse::<f64>() else {
            continue;
        };
        let unit = c.get(3).map(|m| m.as_str());
        let rest = c.get(4).and_then(|m| m.as_str().parse::<f64>().ok());
        let (mut yen, mut tol) = match unit {
            Some("万") => {
                let dec = raw.split_once('.').map_or(0, |(_, d)| {
                    d.split('.').next().unwrap_or("").chars().count()
                });
                (
                    v * 10000.0 + rest.unwrap_or(0.0),
                    if c.get(4).is_some() {
                        1.0
                    } else {
                        10000.0 / 10f64.powi(dec as i32)
                    },
                )
            }
            Some(_) => (v * 1000.0, 1000.0),
            None => (v, 0.0),
        };
        let tail = after(&t, whole.end(), 4);
        // 「月給27〜38万円」: 下限の後ろに単位が無く、上限に「万」が付く書き方
        if unit.is_none()
            && re(&RANGE_MAN, r"^\s*[〜~\-－ー]\s*[0-9]+(?:\.[0-9]+)?\s*万").is_match(after(
                &t,
                whole.end(),
                12,
            ))
        {
            yen = v * 10000.0;
            tol = 10000.0;
        }
        out.push(Amount {
            kind: kind_of(&c[1]),
            yen,
            tol,
            at_least: tail.contains("以上") || tail.starts_with('〜') || tail.starts_with('~'),
            around: format!(
                "{}{}{}",
                before(&t, whole.start(), 8),
                whole.as_str(),
                after(&t, whole.end(), 6)
            ),
            text: whole.as_str().trim().to_string(),
        });
    }
    out
}

/// 3 桁ごとでないカンマ（金額か賞与の月数のものだけ）。
fn comma_bad(text: &str) -> Vec<String> {
    static NUM: OnceLock<Regex> = OnceLock::new();
    static PREV: OnceLock<Regex> = OnceLock::new();
    static UNIT: OnceLock<Regex> = OnceLock::new();
    let src = z2h_keep_comma(text);
    let num = re(&NUM, r"^\d{1,3}(?:,\d+)+");
    let prev_digit = re(&PREV, r"^[\d.]$");
    let mut bad = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let ch = src[i..].chars().next().expect("文字");
        let blocked = src[..i]
            .chars()
            .next_back()
            .is_some_and(|p| prev_digit.is_match(p.encode_utf8(&mut [0; 4])));
        let m = if blocked { None } else { num.find(&src[i..]) };
        let Some(m) = m else {
            i += ch.len_utf8();
            continue;
        };
        let (start, end) = (i, i + m.end());
        let whole = &src[start..end];
        if whole.split(',').skip(1).any(|p| p.chars().count() != 3) {
            let next = after(&src, end, 3);
            if re(&UNIT, r"^\s*(円|万)").is_match(next) || before(&src, start, 8).contains("賞与")
            {
                bad.push(whole.to_string());
            }
        }
        i = end;
    }
    bad
}

fn places(items: Vec<String>) -> String {
    let mut seen = Vec::new();
    for i in items {
        if !seen.contains(&i) {
            seen.push(i);
        }
    }
    let more = seen.len().saturating_sub(MAX_PLACES);
    seen.truncate(MAX_PLACES);
    let mut out = seen.join("／");
    if more > 0 {
        out.push_str(&format!("（ほか {more} か所）"));
    }
    out
}

/// 円の額を表示用に（小数は四捨五入）。
fn yen_f(v: f64) -> String {
    yen(v.round() as i64)
}

/// 有効 4 桁ほどで、末尾の 0 を落とす（8 / 7.75 / 173.3）。
fn short(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// 勤務地の住所の先頭の都道府県らしい語（最低賃金の表に無い語のこともある）。
fn address_prefecture(address: &str) -> Option<String> {
    static PREF: OnceLock<Regex> = OnceLock::new();
    let addr = z2h(address).replace('〒', "");
    re(&PREF, r"(北海道|東京都|京都府|大阪府|.{2,3}県)")
        .captures(&addr)
        .map(|c| c[1].to_string())
}

/// 一覧に出す都道府県（勤務地の住所から。分からなければ無し）。AirWork の求人票には `todoufuken` が無い。
pub fn prefecture(body: &str) -> Option<String> {
    let cols = parse_body(body);
    let address = cols
        .iter()
        .find(|(c, _)| *c == "勤務地")
        .map_or("", |(_, v)| v);
    address_prefecture(address).filter(|p| min_wage(p).is_some())
}

/// 1 件の求人の指摘。`body` は `aw_kyuujinhyou_honbun`。
pub fn check(body: &str) -> Vec<Finding> {
    let cols = parse_body(body);
    let get = |k: &str| {
        cols.iter()
            .find(|(c, _)| *c == k)
            .map_or("", |(_, v)| v.as_str())
    };
    let mut hits: Vec<(Category, String)> = Vec::new();

    // 書きかけの語
    for lab in CHECKED {
        let v = get(lab);
        for w in DRAFT {
            if let Some(i) = v.find(w) {
                hits.push((
                    Category::Placeholder,
                    format!("{lab}「…{}…」", snippet(v, i)),
                ));
            }
        }
    }

    // 文字化け
    static GARBLED: OnceLock<Regex> = OnceLock::new();
    let garbled = re(
        &GARBLED,
        r"[\u{3040}-\u{30FF}\u{4E00}-\u{9FFF}々ー][?\u{FFFD}]+[\u{3040}-\u{30FF}\u{4E00}-\u{9FFF}々ー]",
    );
    for lab in CHECKED {
        let v = get(lab);
        let found: Vec<&str> = garbled.find_iter(v).map(|m| m.as_str()).collect();
        if let Some(first) = found.first() {
            hits.push((
                Category::Mojibake,
                format!("{lab}：{} か所（例「{first}」）", found.len()),
            ));
        }
    }

    // 金額の書き間違い
    for lab in ["キャッチコピー", "給与", "給与の詳細", "仕事内容"] {
        for b in comma_bad(get(lab)) {
            hits.push((Category::AmountTypo, format!("{lab}：カンマの位置「{b}」")));
        }
        for a in free_amounts(get(lab)) {
            if a.yen >= 1e8 {
                hits.push((
                    Category::AmountTypo,
                    format!("{lab}：1 億円以上「{}」", a.text),
                ));
            } else if (a.kind == "月給" || a.kind == "日給") && 0.0 < a.yen && a.yen < 1000.0 {
                hits.push((
                    Category::AmountTypo,
                    format!("{lab}：{}が 3 桁以下「{}」", a.kind, a.text),
                ));
            }
        }
    }

    // 必要な欄が空
    let empty: Vec<&str> = REQUIRED
        .iter()
        .copied()
        .filter(|k| get(k).trim().is_empty())
        .collect();
    if !empty.is_empty() {
        hits.push((
            Category::EmptyFields,
            format!("空の欄：{}", empty.join("・")),
        ));
    }

    // 題名・キャッチコピーの額
    let head = parse_structured(get("給与"));
    if let Some(Pay {
        kind,
        lo: Some(lo),
        hi,
    }) = head.as_ref().filter(|p| p.lo.is_some_and(|v| v != 0.0))
    {
        let top = hi.filter(|h| *h != 0.0 && h >= lo);
        for a in free_amounts(get("キャッチコピー")) {
            if a.kind != *kind || SKIP_NEAR.iter().any(|w| a.around.contains(w)) {
                continue;
            }
            if a.at_least && a.yen <= lo + a.tol {
                continue;
            }
            if a.yen < lo - a.tol || top.is_some_and(|t| a.yen > t + a.tol) {
                let range = match top {
                    Some(t) => format!("{}〜{}", yen_f(*lo), yen_f(t)),
                    None => format!("{}〜", yen_f(*lo)),
                };
                hits.push((
                    Category::HeadlineAmount,
                    format!("キャッチコピーの「{}」、給与は{kind} {range} 円", a.text),
                ));
            }
        }
    }

    // 年間休日の食い違い
    static HOLI: OnceLock<Regex> = OnceLock::new();
    let all = z2h(&CHECKED
        .iter()
        .map(|k| get(k))
        .collect::<Vec<_>>()
        .join("\n"));
    let mut days: Vec<i64> = Vec::new();
    for c in re(
        &HOLI,
        r"年間休日\s*[:：]?\s*(実質)?\s*(\d{2,3})\s*日\s*(以上|〜|～|-|目指)?",
    )
    .captures_iter(&all)
    {
        let whole = c.get(0).expect("一致");
        let next = after(&all, whole.end(), 6);
        // 後ろに何も無い（本文の終わり）も、「＋」が続くのと同じく見送る（試作と同じ）。
        let plus = next.chars().next().is_none_or(|ch| ch == '＋' || ch == '+');
        if c.get(1).is_some()
            || c.get(3).is_some()
            || next.contains("目指")
            || plus
            || before(&all, whole.start(), 6).contains("実質")
        {
            continue;
        }
        if let Ok(d) = c[2].parse::<i64>() {
            if !days.contains(&d) {
                days.push(d);
            }
        }
    }
    if days.len() >= 2 {
        days.sort_unstable();
        let list: Vec<String> = days.iter().map(|d| format!("{d} 日")).collect();
        hits.push((
            Category::HolidayMismatch,
            format!("年間休日が {} と書いてある", list.join("・")),
        ));
    }

    // 種類と額の桁
    let sal = get("給与の詳細");
    // 「基本給：」の行が複数あれば最後の行（読めなければ「給与」の 1 行を使う）。
    let base = sal
        .split('\n')
        .rfind(|line| line.starts_with("基本給："))
        .and_then(parse_structured);
    if let Some(p) = &head {
        if let Some((_, blo, bhi)) = BAND.iter().find(|(k, _, _)| *k == p.kind) {
            let fits = |v: f64| *blo <= v && v <= *bhi;
            let band = format!("{}の目安 {}〜{}", p.kind, yen_f(*blo), yen_f(*bhi));
            if let Some(lo) = p.lo.filter(|v| *v != 0.0 && !fits(*v)) {
                hits.push((
                    Category::TypeBand,
                    format!("給与：{} {} 円（{band}）", p.kind, yen_f(lo)),
                ));
            }
            if let Some(hi) = p.hi.filter(|v| *v != 0.0 && !fits(*v)) {
                hits.push((
                    Category::TypeBand,
                    format!("給与の上限：{} {} 円（{band}）", p.kind, yen_f(hi)),
                ));
            }
        }
        if let (Some(lo), Some(hi)) = (p.lo.filter(|v| *v != 0.0), p.hi.filter(|v| *v != 0.0)) {
            if hi < lo {
                hits.push((
                    Category::TypeBand,
                    format!(
                        "給与：上限 {} 円が下限 {} 円より小さい",
                        yen_f(hi),
                        yen_f(lo)
                    ),
                ));
            }
        }
    }

    // 固定残業代の食い違い
    fixed_overtime(sal, &mut hits);

    // 最低賃金
    let pref = address_prefecture(get("勤務地"));
    let mw = pref.as_deref().and_then(min_wage);
    let src = if base.is_some() {
        "基本給"
    } else {
        "給与"
    };
    let pay = base.or(head);
    if let Some((kind, lo)) = pay
        .as_ref()
        .and_then(|p| p.lo.filter(|v| *v != 0.0).map(|lo| (p.kind, lo)))
    {
        match (mw, pref.as_deref()) {
            (Some(mw), Some(pref)) => {
                if let Some((hourly, how)) = hourly(kind, lo, get("勤務時間")) {
                    if hourly < mw as f64 {
                        hits.push((
                            Category::BelowMinWage,
                            format!(
                                "{src}：{how} = {} 円（{pref} {} 円）",
                                yen_f(hourly),
                                yen(mw)
                            ),
                        ));
                    }
                }
            }
            _ => hits.push((
                Category::UnknownPrefecture,
                "勤務地から都道府県が分からない".to_string(),
            )),
        }
    }

    // 区分ごとに 1 つにまとめる（区分の並びは見つけた順）。
    let mut out: Vec<(Category, Vec<String>)> = Vec::new();
    for (category, detail) in hits {
        match out.iter_mut().find(|(c, _)| *c == category) {
            Some((_, list)) => list.push(detail),
            None => out.push((category, vec![detail])),
        }
    }
    out.into_iter()
        .map(|(category, list)| finding(category, places(list)))
        .collect()
}

/// 時給に直した額と、その求め方。
fn hourly(kind: &str, lo: f64, work: &str) -> Option<(f64, String)> {
    static HOURS: OnceLock<Regex> = OnceLock::new();
    static DAYS: OnceLock<Regex> = OnceLock::new();
    static TOTAL: OnceLock<Regex> = OnceLock::new();
    let hours = re(&HOURS, r"実働時間：1日あたり(\d+)時間(?:(\d+)分)?")
        .captures(work)
        .and_then(|c| {
            let h = c[1].parse::<f64>().ok()?;
            let m = c
                .get(2)
                .map_or(Some(0.0), |m| m.as_str().parse::<f64>().ok())?;
            Some(h + m / 60.0)
        })
        .filter(|h| *h != 0.0);
    let days = re(
        &DAYS,
        r"平均勤務日数：1ヶ月あたり(\d+)日(?:\s*〜\s*(\d+)日)?",
    )
    .captures(work)
    .and_then(|c| {
        let a = c[1].parse::<f64>().ok()?;
        let b = c
            .get(2)
            .map_or(Some(a), |m| m.as_str().parse::<f64>().ok())?;
        Some((a + b) / 2.0)
    })
    .filter(|d| *d != 0.0);
    let total = re(&TOTAL, r"総労働時間：1ヶ月あたり(\d+)時間")
        .captures(work)
        .and_then(|c| c[1].parse::<f64>().ok());
    match kind {
        "時給" => Some((lo, "時給".to_string())),
        "日給" => {
            let h = hours.unwrap_or(8.0);
            Some((lo / h, format!("日給÷{}時間", short(h))))
        }
        _ => {
            let annual = kind == "年収";
            let monthly = if annual { lo / 12.0 } else { lo };
            let (mh, note) = match (hours, days) {
                (Some(h), Some(d)) if (60.0..=260.0).contains(&(h * d)) => {
                    (h * d, format!("（1日{}時間×月{}日）", short(h), short(d)))
                }
                _ => match total.filter(|t| (60.0..=260.0).contains(t)) {
                    Some(t) => (t, "（総労働時間）".to_string()),
                    None => (
                        MONTH_HOURS,
                        "（時間の記載なし、月173.3時間で計算）".to_string(),
                    ),
                },
            };
            Some((
                monthly / mh,
                format!(
                    "{} {} 円÷{:.1}時間{note}",
                    if annual { "年収÷12" } else { "月給" },
                    yen_f(lo),
                    mh
                ),
            ))
        }
    }
}

/// 「固定残業代：あり／なし」の行と、給与の詳細の文の食い違い。
fn fixed_overtime(sal: &str, hits: &mut Vec<(Category, String)>) {
    static NONE_LINE: OnceLock<Regex> = OnceLock::new();
    static YES_LINE: OnceLock<Regex> = OnceLock::new();
    static WORD: OnceLock<Regex> = OnceLock::new();
    static SAYS_NONE: OnceLock<Regex> = OnceLock::new();
    static NOT_INCLUDED: OnceLock<Regex> = OnceLock::new();
    static AMOUNT_OR_INCL: OnceLock<Regex> = OnceLock::new();
    static AMOUNT: OnceLock<Regex> = OnceLock::new();
    static ALL_PAID: OnceLock<Regex> = OnceLock::new();
    let word = re(&WORD, "固定残業|みなし残業|固定時間外|みなし時間外");
    let amount = re(&AMOUNT, r"\d+\s*(時間|h|H)|\d[\d,]*\s*(円|万)");
    let free = sal.rsplit("【一律手当】").next().unwrap_or(sal);
    if re(&NONE_LINE, "(?m)^固定残業代：なし").is_match(sal) {
        for m in word.find_iter(free) {
            let next = z2h(after(free, m.end(), 30));
            if re(
                &SAYS_NONE,
                r"^(代|手当)?\s*[:：は]?\s*(なし|無し|無|ナシ|ありません|は?支給しません|はない|ではない|制度なし|制ではない)",
            )
            .is_match(&next)
            {
                continue;
            }
            if re(
                &NOT_INCLUDED,
                "含み?ません|含まない|含まれない|含まれません|なし|ありません",
            )
            .is_match(&next)
            {
                continue;
            }
            if re(&AMOUNT_OR_INCL, r"\d+\s*(時間|h|H)|\d[\d,]*\s*(円|万)|含").is_match(&next) {
                hits.push((
                    Category::FixedOtMismatch,
                    format!(
                        "固定残業代は「なし」なのに、給与の詳細に「…{}…」",
                        snippet(free, m.start())
                    ),
                ));
                break;
            }
        }
    }
    if let Some(yes) = re(&YES_LINE, "(?m)^固定残業代：あり").find(sal) {
        let next = after(sal, yes.end(), 80);
        let given = next
            .split("\n\n")
            .next()
            .unwrap_or("")
            .contains("1ヶ月あたり");
        let in_free = word
            .find_iter(free)
            .any(|m| amount.is_match(&z2h(after(free, m.end(), 40))));
        if !given && !in_free {
            hits.push((
                Category::FixedOtMismatch,
                "固定残業代は「あり」だが、金額・時間がどこにも書かれていない".to_string(),
            ));
        }
        if !given && re(&ALL_PAID, "(時間外|残業)(手当|代)?は?全額(別途)?支給").is_match(free)
        {
            hits.push((
                Category::FixedOtMismatch,
                "固定残業代は「あり」なのに、給与の詳細に「時間外手当は全額別途支給」".to_string(),
            ));
        }
    }
}
