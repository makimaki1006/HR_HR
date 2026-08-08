//! 給与の Apple-to-Apple 比較のための内訳分解(P1-1)。
//!
//! 求人票の「月給254,200円〜326,000円」は、実際には「基本給221,200円＋残業30〜40時間分」
//! であることがある。この状態で中点290,100円を競合分布に置くと過大訴求になるため、
//! **表示給与 / 基本給 / 固定残業の有無** を分けて持てるようにする。
//!
//! # 判定は機械的に行う(LLM を使わない)
//!
//! 給与は外部公開素材の数値根拠になるため、推測が混ざると訂正できない。
//! ここでは原文の文字列解析だけで判定し、**検出できなかったものは None / false のまま返す**。
//! 推定値の穴埋めは一切しない。`notes` には必ず判定根拠として原文の該当箇所を引用する。
//!
//! # 検出パターン(実データ準拠)
//!
//! - 基本給:
//!   - 「基本給221,200円」「基本給：221,200円」— キーワードの直後14文字以内の金額
//!   - 「221,200円＋残業代」— `＋残業代` の直前で終わる金額(センコー実例。基本給の語が無い)
//! - 固定残業の示唆: 「残業代を含んだ金額」「◯時間分の残業代」「みなし残業」「固定残業」等
//! - 想定残業時間: 「残業 月30〜40時間」「月平均20時間」「20時間分の残業代」
//! - 表示給与: 「月給◯円〜◯円」→ 中点、「月給◯円以上」→ 下限(上限は推測しない)
//!
//! # 誤検出を避けるための制約
//!
//! - 金額は `円` または `万円` を伴うもののみ。裸の数字(「創業19年」「従業員19名」)は拾わない。
//! - 基本給は上記2パターンのどちらかに紐づく金額のみ。単に本文にある金額(交通費上限等)は拾わない。
//! - 残業時間は、数値の前後の文脈(句読点・改行で区切った範囲)に「残業」「みなし」「月平均」が
//!   あるものだけ。「実働8時間」「勤務時間8時間」は拾わない。
//! - 金額・時間とも妥当範囲外(月給3万円未満/300万円超、残業100時間超)は捨てる。
//!
//! # 既知の限界
//!
//! - 時給・日給・年俸表記の月額換算は行わない(`display_monthly_yen` は None)。
//!   単位換算は `crate::handlers::survey::salary_parser` の担当。
//! - 「基本給」が金額より後ろに来る表記(「221,200円(基本給)」)は未対応。
//! - 手当の内訳(住宅手当・皆勤手当等)の分解は対象外。基本給と表示給与の差が
//!   固定残業なのか諸手当なのかまでは判定しない。

use serde::Serialize;

/// 月給として妥当と見なす下限。これ未満は手当・交通費等の別金額とみなして捨てる。
const MIN_PLAUSIBLE_MONTHLY_YEN: i64 = 30_000;
/// 月給として妥当と見なす上限。これ超は年収表記の混入等とみなして捨てる。
const MAX_PLAUSIBLE_MONTHLY_YEN: i64 = 3_000_000;
/// 月あたり残業時間として妥当と見なす上限。これ超は所定労働時間等の誤検出とみなして捨てる。
const MAX_PLAUSIBLE_OVERTIME_HOURS: u32 = 100;

/// 固定残業(みなし残業)が表示給与に含まれることを示唆する語。
/// 出現位置が最も早いものを採用し、同位置なら長いものを優先する。
const FIXED_OVERTIME_MARKERS: [&str; 10] = [
    "残業代を含んだ金額",
    "残業代を含む",
    "残業代を含み",
    "残業代込",
    "残業手当を含",
    "時間分の残業代",
    "固定残業",
    "みなし残業",
    "定額残業",
    "見なし残業",
];

/// 給与表記の内訳分解結果。検出できなかった項目は None / false のまま返す。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SalaryBreakdown {
    /// 基本給(月額・円)。原文に基本給が明示されている場合のみ。
    pub base_monthly_yen: Option<i64>,
    /// 表示給与に残業代が含まれる示唆があるか。
    pub includes_fixed_overtime: bool,
    /// 想定残業時間(月・時間)。単一値の場合は (n, n)。
    pub overtime_hours_range: Option<(u32, u32)>,
    /// 表示給与の代表値(月額・円)。範囲表記は中点、「◯円以上」は下限。
    pub display_monthly_yen: Option<i64>,
    /// 内訳分解に成功したか(基本給と表示給与の両方が取れた場合に true)。
    pub decomposed: bool,
    /// 判定根拠。原文の該当語句を引用して残す。
    pub notes: Vec<String>,
}

/// 給与欄(`salary_text`)と本文(`body_text`)から内訳を分解する。
///
/// 基本給・固定残業・残業時間は給与欄→本文の順に探し、最初に見つかったものを採用する。
/// 表示給与は給与欄のみから取る(本文中の他社比較の金額等を拾わないため)。
pub fn analyze(salary_text: &str, body_text: &str) -> SalaryBreakdown {
    let salary = Norm::new(salary_text);
    let body = Norm::new(body_text);
    let sources: [(&str, &Norm); 2] = [("給与欄", &salary), ("本文", &body)];
    let mut notes: Vec<String> = Vec::new();

    let display_monthly_yen = detect_display(&salary, &mut notes);

    let mut base_monthly_yen = None;
    for (label, source) in sources {
        if let Some(hit) = detect_base(source) {
            notes.push(format!(
                "{label}に基本給の記載あり({}): 「{}」→ {}円",
                hit.rule,
                source.quote(hit.start, hit.end),
                fmt_yen(hit.yen)
            ));
            base_monthly_yen = Some(hit.yen);
            break;
        }
    }

    let mut includes_fixed_overtime = false;
    for (label, source) in sources {
        if let Some((start, end)) = detect_fixed_overtime_marker(source) {
            notes.push(format!(
                "{label}に表示給与へ残業代が含まれる示唆あり: 「{}」",
                source.quote(start, end)
            ));
            includes_fixed_overtime = true;
            break;
        }
    }

    let mut overtime_hours_range = None;
    for (label, source) in sources {
        if let Some((range, start, end)) = detect_overtime_hours(source) {
            notes.push(format!(
                "{label}に想定残業時間の記載あり: 「{}」→ 月{}〜{}時間",
                source.quote(start, end),
                range.0,
                range.1
            ));
            overtime_hours_range = Some(range);
            break;
        }
    }

    let decomposed = base_monthly_yen.is_some() && display_monthly_yen.is_some();
    if !decomposed {
        if includes_fixed_overtime {
            notes.push(
                "残業代が含まれる示唆はあるが基本給の金額が原文にないため、内訳は未分解。\
                 基本給の明示を顧客に確認する必要がある。"
                    .to_string(),
            );
        } else if base_monthly_yen.is_none() {
            notes.push(
                "基本給・固定残業・みなし残業に相当する記載が見つからないため、内訳は未分解。\
                 表示給与の中に固定残業代が含まれているかは原文からは判断できない。"
                    .to_string(),
            );
        }
    } else if let (Some(base), Some(display)) = (base_monthly_yen, display_monthly_yen) {
        notes.push(format!(
            "内訳分解済み: 基本給{}円 / 表示給与{}円(差額{}円)。比較は基本給同士・表示給与同士で行う。",
            fmt_yen(base),
            fmt_yen(display),
            fmt_yen(display - base)
        ));
    }

    SalaryBreakdown {
        base_monthly_yen,
        includes_fixed_overtime,
        overtime_hours_range,
        display_monthly_yen,
        decomposed,
        notes,
    }
}

// ======== 表示給与 ========

/// 給与欄から表示給与の代表値を求める。範囲は中点、「◯円以上」は下限。
fn detect_display(salary: &Norm, notes: &mut Vec<String>) -> Option<i64> {
    if salary.chars.iter().all(|c| c.is_whitespace()) {
        notes.push("給与欄が空のため表示給与を算出できない。".to_string());
        return None;
    }
    let text: String = salary.chars.iter().collect();
    let monthly = ["月給", "月収", "月額", "月俸"]
        .iter()
        .any(|k| text.contains(k));
    if !monthly {
        let other = ["時給", "時間給", "日給", "日当", "週給", "年収", "年俸"]
            .iter()
            .find(|k| text.contains(**k));
        if let Some(unit) = other {
            notes.push(format!(
                "給与欄が月給表記ではない(「{unit}」)ため、月額の代表値は算出しない(単位換算は行わない)。"
            ));
            return None;
        }
    }

    let amounts: Vec<Amount> = scan_amounts(&salary.chars)
        .into_iter()
        .filter(|a| a.has_yen || a.has_man)
        .filter(|a| (MIN_PLAUSIBLE_MONTHLY_YEN..=MAX_PLAUSIBLE_MONTHLY_YEN).contains(&a.yen))
        .collect();
    let low = amounts.first()?;

    // 範囲表記: 直後に区切り文字だけを挟んでもう一つの金額が続く
    if let Some(high) = amounts.get(1) {
        let between = &salary.chars[low.end.min(high.start)..high.start];
        let only_sep = !between.is_empty()
            && between
                .iter()
                .all(|c| c.is_whitespace() || is_range_sep(*c))
            && between.iter().any(|c| is_range_sep(*c));
        if only_sep {
            let mid = (low.yen + high.yen) / 2;
            notes.push(format!(
                "表示給与は範囲表記のため中点を代表値とした: 「{}」→ {}円",
                salary.quote(low.start, high.end),
                fmt_yen(mid)
            ));
            return Some(mid);
        }
    }

    let after = window_after(&salary.chars, low.end, 4);
    if after.starts_with("以上") || after.starts_with("〜") || after.starts_with("～") {
        notes.push(format!(
            "表示給与は下限のみの表記のため下限を代表値とした(上限は推測しない): 「{}」→ {}円",
            salary.quote(low.start, low.end + after.chars().count().min(2)),
            fmt_yen(low.yen)
        ));
        return Some(low.yen);
    }

    notes.push(format!(
        "表示給与: 「{}」→ {}円",
        salary.quote(low.start, low.end),
        fmt_yen(low.yen)
    ));
    Some(low.yen)
}

// ======== 基本給 ========

struct BaseHit {
    yen: i64,
    start: usize,
    end: usize,
    rule: &'static str,
}

/// 「基本給◯円」または「◯円＋残業代」から基本給を取る。
fn detect_base(source: &Norm) -> Option<BaseHit> {
    let amounts = scan_amounts(&source.chars);
    detect_base_by_keyword(source, &amounts)
        .or_else(|| detect_base_before_overtime(source, &amounts))
}

/// 「基本給」「基本給：」の直後14文字以内にある金額。句読点・改行を跨いだら諦める。
fn detect_base_by_keyword(source: &Norm, amounts: &[Amount]) -> Option<BaseHit> {
    for keyword in ["基本給", "基本月給"] {
        let pattern: Vec<char> = keyword.chars().collect();
        let mut from = 0;
        while let Some(pos) = find_chars(&source.chars, &pattern, from) {
            let kw_end = pos + pattern.len();
            from = kw_end;
            let limit = (kw_end + 14).min(source.chars.len());
            if source.chars[kw_end..limit].iter().any(|c| is_delim(*c)) {
                continue;
            }
            let hit = amounts
                .iter()
                .find(|a| a.start >= kw_end && a.start < limit && (a.has_yen || a.has_man))
                .filter(|a| {
                    (MIN_PLAUSIBLE_MONTHLY_YEN..=MAX_PLAUSIBLE_MONTHLY_YEN).contains(&a.yen)
                });
            if let Some(a) = hit {
                return Some(BaseHit {
                    yen: a.yen,
                    start: pos,
                    end: a.end,
                    rule: "「基本給」の直後の金額",
                });
            }
        }
    }
    None
}

/// 「221,200円＋残業代を含んだ金額」のように、`＋残業` の直前で終わる金額。
fn detect_base_before_overtime(source: &Norm, amounts: &[Amount]) -> Option<BaseHit> {
    let pattern: Vec<char> = "残業".chars().collect();
    let mut from = 0;
    while let Some(pos) = find_chars(&source.chars, &pattern, from) {
        from = pos + pattern.len();
        // `＋` は Norm で `+` に正規化済み
        let mut cursor = pos;
        while cursor > 0 && source.chars[cursor - 1].is_whitespace() {
            cursor -= 1;
        }
        if cursor == 0 || source.chars[cursor - 1] != '+' {
            continue;
        }
        cursor -= 1;
        while cursor > 0 && source.chars[cursor - 1].is_whitespace() {
            cursor -= 1;
        }
        // 実データ「※221,200＋残業代を含んだ金額」は円が省略される (2026-08-08 センコー実測)。
        // このパターンに限り単位なしの金額も許可する (妥当レンジのフィルタが誤検出を防ぐ)。
        let hit = amounts
            .iter()
            .find(|a| a.end == cursor)
            .filter(|a| (MIN_PLAUSIBLE_MONTHLY_YEN..=MAX_PLAUSIBLE_MONTHLY_YEN).contains(&a.yen));
        if let Some(a) = hit {
            return Some(BaseHit {
                yen: a.yen,
                start: a.start,
                end: (pos + pattern.len() + 1).min(source.chars.len()),
                rule: "「＋残業代」の直前の金額",
            });
        }
    }
    None
}

// ======== 固定残業 ========

/// 固定残業の示唆語のうち、最も早い位置(同位置なら最長)のものを返す。
fn detect_fixed_overtime_marker(source: &Norm) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for marker in FIXED_OVERTIME_MARKERS {
        let pattern: Vec<char> = marker.chars().collect();
        if let Some(pos) = find_chars(&source.chars, &pattern, 0) {
            let end = pos + pattern.len();
            let better = match best {
                None => true,
                Some((b_start, b_end)) => pos < b_start || (pos == b_start && end > b_end),
            };
            if better {
                best = Some((pos, end));
            }
        }
    }
    best
}

// ======== 想定残業時間 ========

/// 「残業 月30〜40時間」「月平均20時間」「20時間分の残業代」から時間数を取る。
fn detect_overtime_hours(source: &Norm) -> Option<((u32, u32), usize, usize)> {
    let chars = &source.chars;
    let pattern: Vec<char> = "時間".chars().collect();
    let mut from = 0;
    while let Some(pos) = find_chars(chars, &pattern, from) {
        from = pos + pattern.len();

        let mut hi_end = pos;
        while hi_end > 0 && chars[hi_end - 1] == ' ' {
            hi_end -= 1;
        }
        let mut hi_start = hi_end;
        while hi_start > 0 && chars[hi_start - 1].is_ascii_digit() {
            hi_start -= 1;
        }
        if hi_start == hi_end {
            continue;
        }
        let hi: u32 = match chars[hi_start..hi_end].iter().collect::<String>().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        // 「30〜40時間」の下限側
        let mut lo = hi;
        let mut lo_start = hi_start;
        let mut sep_start = hi_start;
        while sep_start > 0 && (chars[sep_start - 1] == ' ' || is_range_sep(chars[sep_start - 1])) {
            sep_start -= 1;
        }
        if sep_start < hi_start && chars[sep_start..hi_start].iter().any(|c| is_range_sep(*c)) {
            let mut cand = sep_start;
            while cand > 0 && chars[cand - 1].is_ascii_digit() {
                cand -= 1;
            }
            if cand < sep_start {
                if let Ok(v) = chars[cand..sep_start]
                    .iter()
                    .collect::<String>()
                    .parse::<u32>()
                {
                    lo = v;
                    lo_start = cand;
                }
            }
        }

        let (lo, hi) = if lo > hi { (hi, lo) } else { (lo, hi) };
        if hi == 0 || hi > MAX_PLAUSIBLE_OVERTIME_HOURS {
            continue;
        }

        let before = window_before(chars, lo_start, 14);
        let after = window_after(chars, pos + pattern.len(), 12);
        let overtime_context = ["残業", "みなし", "見なし", "月平均"]
            .iter()
            .any(|k| before.contains(k) || after.contains(k));
        if !overtime_context {
            continue;
        }
        // 所定労働時間・休憩時間の誤検出を避ける
        let working_hours_context = ["実働", "休憩", "勤務", "拘束", "就業", "所定"]
            .iter()
            .any(|k| before.contains(k));
        if working_hours_context {
            continue;
        }

        return Some(((lo, hi), lo_start, pos + pattern.len()));
    }
    None
}

// ======== 正規化 ========

/// 全角数字・全角カンマ等を吸収した文字列。元テキストの位置を保持し、引用は原文から取る。
struct Norm {
    orig: String,
    chars: Vec<char>,
    /// `chars[i]` に対応する `orig` のバイト位置。
    pos: Vec<usize>,
}

impl Norm {
    fn new(s: &str) -> Self {
        let raw: Vec<(usize, char)> = s.char_indices().collect();
        let mut chars: Vec<char> = Vec::with_capacity(raw.len());
        let mut pos: Vec<usize> = Vec::with_capacity(raw.len());
        for (i, (byte, ch)) in raw.iter().enumerate() {
            let next_is_digit = raw
                .get(i + 1)
                .map(|(_, next)| is_digit_char(*next))
                .unwrap_or(false);
            let prev_is_digit = chars.last().map(|p| p.is_ascii_digit()).unwrap_or(false);
            let mapped = match *ch {
                '０'..='９' => Some(zenkaku_digit(*ch)),
                // 桁区切りのカンマのみ落とす。それ以外は空白扱いにして語の連結を防ぐ
                ',' | '，' => {
                    if prev_is_digit && next_is_digit {
                        None
                    } else {
                        Some(' ')
                    }
                }
                '．' => Some('.'),
                '＋' => Some('+'),
                '　' | '\t' | '\r' => Some(' '),
                other => Some(other),
            };
            if let Some(m) = mapped {
                chars.push(m);
                pos.push(*byte);
            }
        }
        Self {
            orig: s.to_string(),
            chars,
            pos,
        }
    }

    /// 正規化後の `[start, end)` に対応する**原文**を返す(空白は1つに畳む)。
    fn quote(&self, start: usize, end: usize) -> String {
        if start >= end || start >= self.chars.len() {
            return String::new();
        }
        let end = end.min(self.chars.len());
        let from = self.pos[start];
        let to = if end < self.pos.len() {
            self.pos[end]
        } else {
            self.orig.len()
        };
        self.orig[from..to]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn zenkaku_digit(c: char) -> char {
    char::from(b'0' + (c as u32 - '０' as u32) as u8)
}

fn is_digit_char(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c)
}

fn is_range_sep(c: char) -> bool {
    matches!(
        c,
        '〜' | '～' | '~' | '-' | '−' | 'ー' | '－' | '–' | '—' | 'ｰ'
    )
}

fn is_delim(c: char) -> bool {
    matches!(
        c,
        '。' | '、' | '\n' | '｜' | '|' | '【' | '】' | '＜' | '＞'
    )
}

fn find_chars(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

/// `start` の手前 `max_len` 文字。句読点・改行があればそこで切る(直近の節だけ見る)。
fn window_before(chars: &[char], start: usize, max_len: usize) -> String {
    let lo = start.saturating_sub(max_len);
    let mut out = String::new();
    for &c in &chars[lo..start] {
        if is_delim(c) {
            out.clear();
        } else {
            out.push(c);
        }
    }
    out
}

/// `end` の直後 `max_len` 文字。句読点・改行で止める。
fn window_after(chars: &[char], end: usize, max_len: usize) -> String {
    if end >= chars.len() {
        return String::new();
    }
    let hi = (end + max_len).min(chars.len());
    let mut out = String::new();
    for &c in &chars[end..hi] {
        if is_delim(c) {
            break;
        }
        out.push(c);
    }
    out
}

// ======== 金額スキャン ========

#[derive(Debug, Clone)]
struct Amount {
    yen: i64,
    start: usize,
    /// 「円」「万円」まで含めた終端(排他)。
    end: usize,
    has_man: bool,
    has_yen: bool,
}

/// 数値 + 任意の「万」「円」を金額として拾う。単位を伴わないものも位置把握のため返す
/// (基本給・表示給与の判定側で `has_yen || has_man` を必須にしている)。
fn scan_amounts(chars: &[char]) -> Vec<Amount> {
    let mut out: Vec<Amount> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j + 1 < chars.len() && chars[j] == '.' && chars[j + 1].is_ascii_digit() {
            j += 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
        }
        let number: f64 = match chars[start..j].iter().collect::<String>().parse() {
            Ok(v) => v,
            Err(_) => {
                i = j.max(start + 1);
                continue;
            }
        };
        let mut k = j;
        while k < chars.len() && chars[k] == ' ' {
            k += 1;
        }
        let mut has_man = false;
        if k < chars.len() && chars[k] == '万' {
            has_man = true;
            k += 1;
            while k < chars.len() && chars[k] == ' ' {
                k += 1;
            }
        }
        let mut has_yen = false;
        if k < chars.len() && chars[k] == '円' {
            has_yen = true;
            k += 1;
        }
        let end = if has_man || has_yen { k } else { j };
        let yen = (number * if has_man { 10_000.0 } else { 1.0 }).round() as i64;
        out.push(Amount {
            yen,
            start,
            end,
            has_man,
            has_yen,
        });
        i = end.max(j).max(start + 1);
    }

    // 「25〜30万円」のように、前側に単位が付かない範囲表記の単位を補う
    for idx in 0..out.len().saturating_sub(1) {
        let (left, right) = out.split_at_mut(idx + 1);
        let a = &mut left[idx];
        let b = &right[0];
        if a.has_man || a.has_yen || !b.has_man || a.end >= b.start {
            continue;
        }
        let between = &chars[a.end..b.start];
        if between.iter().all(|c| *c == ' ' || is_range_sep(*c))
            && between.iter().any(|c| is_range_sep(*c))
        {
            a.yen *= 10_000;
            a.has_man = true;
        }
    }
    out
}

fn fmt_yen(value: i64) -> String {
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if value < 0 {
        format!("-{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実例(センコー): 表示給与は範囲だが、本文に「221,200円＋残業代を含んだ金額」とある。
    #[test]
    fn decomposes_senko_style_fixed_overtime_salary() {
        let salary = "月給254,200円〜326,000円";
        let body = "上記月給は、221,200円＋残業代を含んだ金額です。残業 月30〜40時間程度。";
        let breakdown_no_yen = analyze(
            "月給254,200円〜326,000円",
            "※221,200＋残業代を含んだ金額 ※固定残業なし",
        );
        assert_eq!(
            breakdown_no_yen.base_monthly_yen,
            Some(221_200),
            "円省略表記 (実HTMLの原文ママ) で基本給を検出できるべき"
        );
        let result = analyze(salary, body);

        assert_eq!(result.base_monthly_yen, Some(221_200));
        assert!(result.includes_fixed_overtime);
        assert_eq!(result.overtime_hours_range, Some((30, 40)));
        assert_eq!(result.display_monthly_yen, Some(290_100));
        assert!(result.decomposed);
        // 判定根拠に原文の該当語句が引用されている
        let notes = result.notes.join("\n");
        assert!(notes.contains("221,200円"), "notes={notes}");
        assert!(notes.contains("残業代を含んだ金額"), "notes={notes}");
        assert!(notes.contains("30〜40時間"), "notes={notes}");
    }

    /// 内訳の記載がない単純な月給: 表示給与だけ算出し、分解は未達とする。
    #[test]
    fn plain_monthly_salary_is_not_decomposed() {
        let result = analyze("月給250,000円", "未経験歓迎。研修制度あり。");

        assert_eq!(result.display_monthly_yen, Some(250_000));
        assert_eq!(result.base_monthly_yen, None);
        assert!(!result.includes_fixed_overtime);
        assert_eq!(result.overtime_hours_range, None);
        assert!(!result.decomposed);
        assert!(result.notes.iter().any(|n| n.contains("未分解")));
    }

    /// 「◯円以上」は下限を代表値にする(上限を推測しない)。
    #[test]
    fn open_ended_salary_uses_lower_bound() {
        let result = analyze("月給30万円以上", "");

        assert_eq!(result.display_monthly_yen, Some(300_000));
        assert!(result.notes.iter().any(|n| n.contains("下限")));
    }

    /// 範囲表記の中点。
    #[test]
    fn range_salary_uses_midpoint() {
        let result = analyze("月給200,000円〜300,000円", "");
        assert_eq!(result.display_monthly_yen, Some(250_000));
    }

    /// 単位が後ろにしか付かない範囲表記「25〜30万円」。
    #[test]
    fn range_with_trailing_unit_only() {
        let result = analyze("月給25〜30万円", "");
        assert_eq!(result.display_monthly_yen, Some(275_000));
    }

    /// 全角数字・全角カンマ・全角プラス・全角スペースの混在。
    #[test]
    fn handles_fullwidth_digits_and_punctuation() {
        let salary = "月給２５４，２００円〜３２６，０００円";
        let body = "基本給２２１，２００円＋残業代を含んだ金額です。残業　月３０〜４０時間";
        let result = analyze(salary, body);

        assert_eq!(result.display_monthly_yen, Some(290_100));
        assert_eq!(result.base_monthly_yen, Some(221_200));
        assert!(result.includes_fixed_overtime);
        assert_eq!(result.overtime_hours_range, Some((30, 40)));
        assert!(result.decomposed);
        // 引用は原文のまま(全角)であること
        assert!(
            result.notes.iter().any(|n| n.contains("２２１，２００円")),
            "notes={:?}",
            result.notes
        );
    }

    /// 逆証明: 本文の無関係な数字を基本給・残業時間と誤認しない。
    #[test]
    fn does_not_mistake_unrelated_numbers_for_breakdown() {
        let salary = "月給254,200円〜326,000円";
        let body = "創業19年、従業員19名の安定企業です。交通費上限15,000円。\
                    実働8時間、休憩60分。残業ほぼなし。年間休日120日。";
        let result = analyze(salary, body);

        assert_eq!(result.base_monthly_yen, None, "notes={:?}", result.notes);
        assert_eq!(
            result.overtime_hours_range, None,
            "notes={:?}",
            result.notes
        );
        assert!(!result.includes_fixed_overtime);
        assert!(!result.decomposed);
        assert_eq!(result.display_monthly_yen, Some(290_100));
    }

    /// 逆証明: 所定労働時間は残業時間として拾わない。
    #[test]
    fn working_hours_are_not_overtime_hours() {
        let result = analyze(
            "月給250,000円",
            "勤務時間は実働8時間です。残業は繁忙期のみ。",
        );
        assert_eq!(
            result.overtime_hours_range, None,
            "notes={:?}",
            result.notes
        );
    }

    /// 「基本給：」のようにコロン付きでも拾う。
    #[test]
    fn base_salary_with_colon() {
        let result = analyze("月給280,000円", "内訳 基本給：230,000円 職務手当50,000円");
        assert_eq!(result.base_monthly_yen, Some(230_000));
        assert!(result.decomposed);
    }

    /// みなし残業の時間数のみが書かれているケース。基本給が無いので未分解のまま。
    #[test]
    fn deemed_overtime_hours_without_base_salary() {
        let result = analyze("月給300,000円", "みなし残業20時間分を含みます。");

        assert!(result.includes_fixed_overtime);
        assert_eq!(result.overtime_hours_range, Some((20, 20)));
        assert_eq!(result.base_monthly_yen, None);
        assert!(!result.decomposed);
        assert!(result.notes.iter().any(|n| n.contains("未分解")));
    }

    /// 「◯時間分の残業代」は数値の後ろの文脈から残業時間と判定する。
    #[test]
    fn overtime_hours_from_trailing_context() {
        let result = analyze("月給300,000円", "45時間分の残業代を含みます。");
        assert_eq!(result.overtime_hours_range, Some((45, 45)));
        assert!(result.includes_fixed_overtime);
    }

    /// 時給表記は月額換算しない(単位換算は別モジュールの担当)。
    #[test]
    fn hourly_salary_is_not_converted() {
        let result = analyze("時給1,200円", "");
        assert_eq!(result.display_monthly_yen, None);
        assert!(result.notes.iter().any(|n| n.contains("時給")));
    }

    /// 給与欄が空でもパニックせず、None を返す。
    #[test]
    fn empty_salary_text_is_safe() {
        let result = analyze("", "");
        assert_eq!(result.display_monthly_yen, None);
        assert_eq!(result.base_monthly_yen, None);
        assert!(!result.decomposed);
    }

    /// 「月平均◯時間」の表記。
    #[test]
    fn monthly_average_overtime_hours() {
        let result = analyze("月給250,000円", "残業は月平均15時間です。");
        assert_eq!(result.overtime_hours_range, Some((15, 15)));
    }

    #[test]
    fn formats_yen_with_separators() {
        assert_eq!(fmt_yen(0), "0");
        assert_eq!(fmt_yen(999), "999");
        assert_eq!(fmt_yen(1_000), "1,000");
        assert_eq!(fmt_yen(221_200), "221,200");
        assert_eq!(fmt_yen(-1_234_567), "-1,234,567");
    }
}
