//! 給与の Apple-to-Apple 比較のための内訳分解(P1-1)。
//!
//! 求人票の「月給254,200円〜326,000円」は、実際には「基本給221,200円＋残業代を含んだ想定月収」
//! であることがある。この状態で中点290,100円を競合分布に置くと過大訴求になるため、
//! **表示月給(下限/上限/中点) / 基本給 / 固定残業制度の有無 / 表示に残業代を含むか** を
//! 分けて持てるようにする。
//!
//! # 「固定残業」と「表示に残業代を含む」は別概念
//!
//! この2つを1つのフラグにまとめると意味が反転するため、必ず別々に持つ。
//!
//! - `fixed_overtime`: **固定残業(みなし残業)制度**の有無。一定時間分の残業代を
//!   実際の残業時間に関わらず定額で支払う制度があるか。原文に「固定残業なし」と
//!   書かれていれば `Some(false)`。
//! - `overtime_pay_included_in_display`: 表示月給(またはその例)が
//!   **実残業代を含んだ金額**として提示されているか。固定残業制度が無くても真になる。
//!
//! 実例(センコー): 「月給254,200円〜326,000円」「221,200＋残業代を含んだ金額」「固定残業なし」
//! → `fixed_overtime = Some(false)` かつ `overtime_pay_included_in_display = true`。
//! これは「固定残業代を含む月給」ではなく「基本給＋実残業代を含んだ想定月収レンジ」である。
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
//!   - 「221,200円＋残業代」— `＋残業` の直前で終わる金額(センコー実例。基本給の語が無い)
//! - 固定残業制度: 「固定残業」「みなし残業」「定額残業」の出現。ただし直後20文字以内に
//!   「なし」「ありません」「無し」があれば**否定を最優先**して `Some(false)`。
//! - 表示に残業代を含む: 「残業代を含んだ金額」「残業代を含む」「残業代込」「残業手当を含」
//!   「◯時間分の残業代」
//! - 想定残業時間: 「残業 月30〜40時間」「月平均20時間」「20時間分の残業代」
//! - 表示月給: 「月給A円〜B円」→ 下限A/上限B/中点(A+B)/2、「月給A円以上」→ 下限Aのみ
//!   (上限・中点は推測しない)
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
//! - 時給・日給・年俸表記の月額換算は行わない(`display_monthly_*_yen` は None)。
//!   単位換算は `crate::handlers::survey::salary_parser` の担当。
//! - 「基本給」が金額より後ろに来る表記(「221,200円(基本給)」)は未対応。
//! - **基本給と表示月給の差額の内訳は一切判定しない**。差が残業代なのか、諸手当
//!   (住宅手当・職務手当・皆勤手当等)なのか、その混合なのかは原文からは決まらない。
//!   差を「残業代」と呼ぶ出力を作ってはならない。
//! - 上限側の金額が何を前提にしているか(残業上限時・経験者採用時等)も判定しない。

use serde::Serialize;

/// 月給として妥当と見なす下限。これ未満は手当・交通費等の別金額とみなして捨てる。
const MIN_PLAUSIBLE_MONTHLY_YEN: i64 = 30_000;
/// 月給として妥当と見なす上限。これ超は年収表記の混入等とみなして捨てる。
const MAX_PLAUSIBLE_MONTHLY_YEN: i64 = 3_000_000;
/// 月あたり残業時間として妥当と見なす上限。これ超は所定労働時間等の誤検出とみなして捨てる。
const MAX_PLAUSIBLE_OVERTIME_HOURS: u32 = 100;

/// 表示月給に**実残業代が含まれる**旨を示す語。固定残業制度の有無とは無関係。
/// 出現位置が最も早いものを採用し、同位置なら長いものを優先する。
const OVERTIME_INCLUDED_MARKERS: [&str; 6] = [
    "残業代を含んだ金額",
    "残業代を含む",
    "残業代を含み",
    "残業代込",
    "残業手当を含",
    "時間分の残業代",
];

/// 固定残業(みなし残業)**制度**を指す語。肯定・否定いずれの判定にも使う。
const FIXED_OVERTIME_SYSTEM_MARKERS: [&str; 4] =
    ["固定残業", "みなし残業", "定額残業", "見なし残業"];

/// 制度語の直後にあれば「制度なし」と判定する語。
const NEGATION_WORDS: [&str; 3] = ["なし", "ありません", "無し"];

/// 制度語の直後、否定語を探す範囲(文字数)。句読点・改行があればそこで打ち切る。
const NEGATION_WINDOW_CHARS: usize = 20;

/// 給与表記の内訳分解結果。検出できなかった項目は None / false のまま返す。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SalaryBreakdown {
    /// 基本給(月額・円)。原文に基本給が明示されている場合のみ。
    pub base_monthly_yen: Option<i64>,
    /// 表示月給の下限(円)。範囲表記「A円〜B円」なら A、「A円以上」なら A。
    pub display_monthly_min_yen: Option<i64>,
    /// 表示月給の上限(円)。「A円以上」のように上限が書かれていない場合は None(推測しない)。
    pub display_monthly_max_yen: Option<i64>,
    /// 表示月給の中点(円)。上限が取れた場合のみ `(min + max) / 2`。
    /// **中点であって下限ではない**。単独訴求に使うと過大表示になる。
    pub display_monthly_midpoint_yen: Option<i64>,
    /// 固定残業(みなし残業)制度の有無。
    /// 「固定残業なし」等の明示的な否定があれば `Some(false)`(**最優先**)。
    /// 「固定残業」「みなし残業」「定額残業」の肯定文脈があれば `Some(true)`。
    /// どちらの記載も無ければ `None`(推測しない)。
    pub fixed_overtime: Option<bool>,
    /// 表示月給(またはその例)に残業代が含まれる旨の記載があるか。
    /// 「残業代を含んだ金額」「残業代込」等。**固定残業制度の有無とは独立**。
    pub overtime_pay_included_in_display: bool,
    /// 想定残業時間(月・時間)。単一値の場合は (n, n)。
    pub overtime_hours_range: Option<(u32, u32)>,
    /// 内訳分解に成功したか(基本給と表示月給の両方が取れた場合に true)。
    pub decomposed: bool,
    /// 判定根拠。原文の該当語句を引用して残す。
    pub notes: Vec<String>,
}

/// 給与欄(`salary_text`)と本文(`body_text`)から内訳を分解する。
///
/// 基本給・固定残業・残業時間は給与欄→本文の順に探し、最初に見つかったものを採用する。
/// ただし固定残業だけは**否定表現を全ソースで先に探す**(否定が最優先)。
/// 表示月給は給与欄のみから取る(本文中の他社比較の金額等を拾わないため)。
pub fn analyze(salary_text: &str, body_text: &str) -> SalaryBreakdown {
    let salary = Norm::new(salary_text);
    let body = Norm::new(body_text);
    let sources: [(&str, &Norm); 2] = [("給与欄", &salary), ("本文", &body)];
    let mut notes: Vec<String> = Vec::new();

    let display = detect_display(&salary, &mut notes);
    let display_monthly_min_yen = display.as_ref().map(|d| d.min);
    let display_monthly_max_yen = display.as_ref().and_then(|d| d.max);
    let display_monthly_midpoint_yen = display.as_ref().and_then(|d| d.midpoint);

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

    // 固定残業制度: 否定 → 肯定 の順。否定があれば肯定マーカーは無視する。
    let mut fixed_overtime: Option<bool> = None;
    for (label, source) in sources {
        if let Some((start, end)) = detect_fixed_overtime_negation(source) {
            notes.push(format!(
                "{label}に固定残業(みなし残業)を否定する記載あり: 「{}」→ 固定残業制度は無いと判定。",
                source.quote(start, end)
            ));
            fixed_overtime = Some(false);
            break;
        }
    }
    if fixed_overtime.is_none() {
        for (label, source) in sources {
            if let Some((start, end)) = detect_fixed_overtime_positive(source) {
                notes.push(format!(
                    "{label}に固定残業(みなし残業)制度の記載あり: 「{}」",
                    source.quote(start, end)
                ));
                fixed_overtime = Some(true);
                break;
            }
        }
    }
    if fixed_overtime.is_none() {
        notes.push(
            "固定残業(みなし残業)制度の有無に関する記載が原文に無いため、有無は判定しない。"
                .to_string(),
        );
    }

    let mut overtime_pay_included_in_display = false;
    for (label, source) in sources {
        if let Some((start, end)) = detect_overtime_included_marker(source) {
            notes.push(format!(
                "{label}に表示月給へ残業代が含まれる旨の記載あり: 「{}」",
                source.quote(start, end)
            ));
            overtime_pay_included_in_display = true;
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

    let decomposed = base_monthly_yen.is_some() && display_monthly_min_yen.is_some();
    if !decomposed {
        if overtime_pay_included_in_display {
            notes.push(
                "表示月給に残業代が含まれる旨の記載はあるが基本給の金額が原文にないため、内訳は未分解。\
                 基本給の明示を顧客に確認する必要がある。"
                    .to_string(),
            );
        } else if base_monthly_yen.is_none() {
            notes.push(
                "基本給に相当する金額の記載が見つからないため、内訳は未分解。\
                 表示月給の内訳(基本給・手当・残業代の別)は原文からは判断できない。"
                    .to_string(),
            );
        }
    } else if let (Some(base), Some(min)) = (base_monthly_yen, display_monthly_min_yen) {
        let display_desc = match (display_monthly_max_yen, display_monthly_midpoint_yen) {
            (Some(max), Some(mid)) => format!(
                "表示月給{}〜{}円(中点{}円)",
                fmt_yen(min),
                fmt_yen(max),
                fmt_yen(mid)
            ),
            _ => format!("表示月給{}円以上", fmt_yen(min)),
        };
        notes.push(format!(
            "内訳分解済み: 基本給{}円 / {display_desc}。比較は基本給同士・表示月給同士で行う。\
             基本給と表示月給の開きが何によるもの(諸手当・残業代・その他)かは原文からは判定しない。",
            fmt_yen(base)
        ));
    }

    SalaryBreakdown {
        base_monthly_yen,
        display_monthly_min_yen,
        display_monthly_max_yen,
        display_monthly_midpoint_yen,
        fixed_overtime,
        overtime_pay_included_in_display,
        overtime_hours_range,
        decomposed,
        notes,
    }
}

// ======== 表示月給 ========

/// 表示月給の下限・上限・中点。上限が原文に無い場合 `max` / `midpoint` は None。
struct DisplayRange {
    min: i64,
    max: Option<i64>,
    midpoint: Option<i64>,
}

/// 給与欄から表示月給の下限・上限・中点を求める。
fn detect_display(salary: &Norm, notes: &mut Vec<String>) -> Option<DisplayRange> {
    if salary.chars.iter().all(|c| c.is_whitespace()) {
        notes.push("給与欄が空のため表示月給を算出できない。".to_string());
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
                "給与欄が月給表記ではない(「{unit}」)ため、月額は算出しない(単位換算は行わない)。"
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
                "表示月給は範囲表記: 「{}」→ 下限{}円 / 上限{}円 / 中点{}円(中点は代表値であって下限ではない)",
                salary.quote(low.start, high.end),
                fmt_yen(low.yen),
                fmt_yen(high.yen),
                fmt_yen(mid)
            ));
            return Some(DisplayRange {
                min: low.yen,
                max: Some(high.yen),
                midpoint: Some(mid),
            });
        }
    }

    let after = window_after(&salary.chars, low.end, 4);
    if after.starts_with("以上") || after.starts_with("〜") || after.starts_with("～") {
        notes.push(format!(
            "表示月給は下限のみの表記のため下限だけを採用した(上限・中点は推測しない): 「{}」→ 下限{}円",
            salary.quote(low.start, low.end + after.chars().count().min(2)),
            fmt_yen(low.yen)
        ));
        return Some(DisplayRange {
            min: low.yen,
            max: None,
            midpoint: None,
        });
    }

    notes.push(format!(
        "表示月給は単一額の表記: 「{}」→ {}円",
        salary.quote(low.start, low.end),
        fmt_yen(low.yen)
    ));
    Some(DisplayRange {
        min: low.yen,
        max: Some(low.yen),
        midpoint: Some(low.yen),
    })
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

// ======== 固定残業(みなし残業)制度 ========

/// 「固定残業なし」「みなし残業代はありません」等、制度の**明示的な否定**を探す。
///
/// 制度語の直後 `NEGATION_WINDOW_CHARS` 文字以内(句読点・改行で打ち切り)に否定語があれば
/// 該当とみなす。返す範囲は制度語の開始から否定語の終端まで(引用にそのまま使える)。
fn detect_fixed_overtime_negation(source: &Norm) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for marker in FIXED_OVERTIME_SYSTEM_MARKERS {
        let pattern: Vec<char> = marker.chars().collect();
        let mut from = 0;
        while let Some(pos) = find_chars(&source.chars, &pattern, from) {
            let marker_end = pos + pattern.len();
            from = marker_end;
            let window: Vec<char> = window_after(&source.chars, marker_end, NEGATION_WINDOW_CHARS)
                .chars()
                .collect();
            let hit = NEGATION_WORDS.iter().filter_map(|word| {
                let needle: Vec<char> = word.chars().collect();
                find_chars(&window, &needle, 0).map(|idx| idx + needle.len())
            });
            if let Some(rel_end) = hit.min() {
                let end = marker_end + rel_end;
                let better = match best {
                    None => true,
                    Some((b_start, _)) => pos < b_start,
                };
                if better {
                    best = Some((pos, end));
                }
            }
        }
    }
    best
}

/// 否定を伴わない固定残業(みなし残業)制度の記載を探す。最も早い位置のものを返す。
fn detect_fixed_overtime_positive(source: &Norm) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for marker in FIXED_OVERTIME_SYSTEM_MARKERS {
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

// ======== 表示月給に残業代を含むか ========

/// 「残業代を含んだ金額」等の語のうち、最も早い位置(同位置なら最長)のものを返す。
/// 固定残業制度の語(固定残業/みなし残業/定額残業)はここでは使わない。
fn detect_overtime_included_marker(source: &Norm) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for marker in OVERTIME_INCLUDED_MARKERS {
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
    // 「時間」に加えて「h」単位表記 (実データ例:「残業あり（月30～40h）」) も対象にする。
    // h は残業文脈 (overtime_context) の必須判定があるため、URL等の h を誤検出しない。
    // 給与例の「残業30時間」(単一値) より、勤務時間欄の「月30〜40h」(範囲) を優先する:
    // 範囲表記の候補があればそれを採用し、無ければ最初の単一値を採用する。
    let mut first_single: Option<((u32, u32), usize, usize)> = None;
    for unit in ["時間", "h", "H", "ｈ", "Ｈ"] {
        let mut from = 0;
        while let Some(hit) = detect_overtime_hours_with_unit_from(source, unit, from) {
            let ((lo, hi), start, end) = hit;
            if lo < hi {
                return Some(hit);
            }
            if first_single.is_none() {
                first_single = Some(((lo, hi), start, end));
            }
            from = end;
        }
    }
    first_single
}

fn detect_overtime_hours_with_unit_from(
    source: &Norm,
    unit: &str,
    start_from: usize,
) -> Option<((u32, u32), usize, usize)> {
    let chars = &source.chars;
    let pattern: Vec<char> = unit.chars().collect();
    let mut from = start_from;
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
        // 所定労働時間・休憩時間の誤検出を避ける。ただし「休憩75分※残業あり（月30〜40h）」の
        // ように残業語の方が数値に近い場合は残業として扱う (数値に近いキーワードを優先)。
        let last_pos =
            |keys: &[&str]| -> Option<usize> { keys.iter().filter_map(|k| before.rfind(k)).max() };
        let overtime_pos = last_pos(&["残業", "みなし", "見なし", "月平均"]);
        let working_pos = last_pos(&["実働", "休憩", "勤務", "拘束", "就業", "所定"]);
        if let Some(w) = working_pos {
            if overtime_pos.is_none_or(|o| o < w) {
                continue;
            }
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
/// (基本給・表示月給の判定側で `has_yen || has_man` を必須にしている)。
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
    /// 給与例の単一値「残業30時間」より勤務時間欄の範囲「月30～40h」を優先する。
    #[test]
    fn overtime_hours_prefers_range_over_single_example() {
        let b = analyze(
            "月給254,200円〜326,000円",
            "給与例: 1年目 月22日勤務／残業30時間。勤務時間: 07:00~15:45※残業あり（月30～40h）",
        );
        assert_eq!(b.overtime_hours_range, Some((30, 40)), "{:?}", b.notes);
    }

    /// 実データ表記「※残業あり（月30～40h）」(h単位+全角チルダ) の検出 (2026-08-08 Ledger化)。
    #[test]
    fn overtime_hours_with_h_unit_and_fullwidth_tilde() {
        let b = analyze(
            "月給254,200円〜326,000円",
            "07:00~15:45《日勤》※実働7時間30分＋休憩75分※残業あり（月30～40h）",
        );
        assert_eq!(b.overtime_hours_range, Some((30, 40)), "{:?}", b.notes);
    }

    use super::*;

    /// 実例(センコー)フル: 表示は範囲、本文に「221,200＋残業代を含んだ金額」「固定残業なし」
    /// 「残業 月30〜40時間」。**固定残業は無い**が表示には実残業代が含まれる、という組み合わせ。
    #[test]
    fn senko_full_example_has_no_fixed_overtime_but_includes_overtime_pay() {
        let result = analyze(
            "月給254,200円〜326,000円",
            "※221,200＋残業代を含んだ金額 ※固定残業なし ※残業 月30〜40時間",
        );

        assert_eq!(result.base_monthly_yen, Some(221_200), "{:?}", result.notes);
        assert_eq!(result.display_monthly_min_yen, Some(254_200));
        assert_eq!(result.display_monthly_max_yen, Some(326_000));
        assert_eq!(result.display_monthly_midpoint_yen, Some(290_100));
        assert_eq!(
            result.fixed_overtime,
            Some(false),
            "「固定残業なし」は明示否定として最優先されるべき: {:?}",
            result.notes
        );
        assert!(result.overtime_pay_included_in_display);
        assert_eq!(result.overtime_hours_range, Some((30, 40)));
        assert!(result.decomposed);

        let notes = result.notes.join("\n");
        assert!(notes.contains("221,200"), "notes={notes}");
        assert!(notes.contains("残業代を含んだ金額"), "notes={notes}");
        assert!(notes.contains("固定残業なし"), "notes={notes}");
        assert!(notes.contains("30〜40時間"), "notes={notes}");
    }

    /// 逆証明(センコー実例): 基本給と表示月給の差(290,100-221,200=68,900)を
    /// 「残業代」「差額」と断定する文言を notes に出してはならない。
    #[test]
    fn notes_never_attribute_the_gap_to_overtime_pay() {
        let result = analyze(
            "月給254,200円〜326,000円",
            "※221,200＋残業代を含んだ金額 ※固定残業なし ※残業 月30〜40時間",
        );
        let notes = result.notes.join("\n");

        for forbidden in ["68,900", "68900", "差額", "＝残業代", "=残業代"] {
            assert!(
                !notes.contains(forbidden),
                "notes に「{forbidden}」が含まれてはならない: {notes}"
            );
        }
    }

    /// 固定残業の明示否定は、同じ本文に肯定マーカーがあっても優先される。
    #[test]
    fn explicit_negation_wins_over_positive_marker() {
        let result = analyze(
            "月給300,000円",
            "みなし残業手当の制度はありません。固定残業なし。",
        );
        assert_eq!(result.fixed_overtime, Some(false), "{:?}", result.notes);
    }

    /// 固定残業(みなし残業)制度の肯定記載。
    #[test]
    fn deemed_overtime_marker_sets_fixed_overtime_true() {
        let result = analyze("月給300,000円", "みなし残業20時間分を含みます。");

        assert_eq!(result.fixed_overtime, Some(true));
        assert_eq!(result.overtime_hours_range, Some((20, 20)));
        // 制度語だけでは「表示額に残業代を含む」とは言えない(概念が別)
        assert!(
            !result.overtime_pay_included_in_display,
            "{:?}",
            result.notes
        );
        assert_eq!(result.base_monthly_yen, None);
        assert!(!result.decomposed);
        assert!(result.notes.iter().any(|n| n.contains("未分解")));
    }

    /// 固定残業肯定 かつ 表示額に残業代を含む、の両立ケース。
    #[test]
    fn fixed_overtime_and_included_pay_can_both_be_true() {
        let result = analyze("月給300,000円", "みなし残業20時間分の残業代を含みます。");

        assert_eq!(result.fixed_overtime, Some(true));
        assert!(result.overtime_pay_included_in_display);
    }

    /// 記載が無ければ固定残業の有無は判定しない(false と断定しない)。
    #[test]
    fn absent_fixed_overtime_description_is_none() {
        let result = analyze("月給250,000円", "未経験歓迎。研修制度あり。");

        assert_eq!(result.fixed_overtime, None);
        assert!(result
            .notes
            .iter()
            .any(|n| n.contains("記載が原文に無いため")));
    }

    /// 内訳の記載がない単純な月給: 表示月給だけ算出し、分解は未達とする。
    #[test]
    fn plain_monthly_salary_is_not_decomposed() {
        let result = analyze("月給250,000円", "未経験歓迎。研修制度あり。");

        assert_eq!(result.display_monthly_min_yen, Some(250_000));
        assert_eq!(result.display_monthly_max_yen, Some(250_000));
        assert_eq!(result.display_monthly_midpoint_yen, Some(250_000));
        assert_eq!(result.base_monthly_yen, None);
        assert!(!result.overtime_pay_included_in_display);
        assert_eq!(result.overtime_hours_range, None);
        assert!(!result.decomposed);
        assert!(result.notes.iter().any(|n| n.contains("未分解")));
    }

    /// 「◯円以上」は下限のみ。上限・中点は推測しない。
    #[test]
    fn open_ended_salary_reports_min_only() {
        let result = analyze("月給30万円以上", "");

        assert_eq!(result.display_monthly_min_yen, Some(300_000));
        assert_eq!(result.display_monthly_max_yen, None);
        assert_eq!(result.display_monthly_midpoint_yen, None);
        assert!(result.notes.iter().any(|n| n.contains("下限のみの表記")));
    }

    /// 範囲表記は下限・上限・中点の3点を返す。
    #[test]
    fn range_salary_reports_min_max_and_midpoint() {
        let result = analyze("月給200,000円〜300,000円", "");

        assert_eq!(result.display_monthly_min_yen, Some(200_000));
        assert_eq!(result.display_monthly_max_yen, Some(300_000));
        assert_eq!(result.display_monthly_midpoint_yen, Some(250_000));
        // 中点を「下限」と呼ばないこと
        let notes = result.notes.join("\n");
        assert!(notes.contains("中点"), "notes={notes}");
    }

    /// 単位が後ろにしか付かない範囲表記「25〜30万円」。
    #[test]
    fn range_with_trailing_unit_only() {
        let result = analyze("月給25〜30万円", "");

        assert_eq!(result.display_monthly_min_yen, Some(250_000));
        assert_eq!(result.display_monthly_max_yen, Some(300_000));
        assert_eq!(result.display_monthly_midpoint_yen, Some(275_000));
    }

    /// 全角数字・全角カンマ・全角プラス・全角スペースの混在。
    #[test]
    fn handles_fullwidth_digits_and_punctuation() {
        let salary = "月給２５４，２００円〜３２６，０００円";
        let body = "基本給２２１，２００円＋残業代を含んだ金額です。残業　月３０〜４０時間";
        let result = analyze(salary, body);

        assert_eq!(result.display_monthly_min_yen, Some(254_200));
        assert_eq!(result.display_monthly_max_yen, Some(326_000));
        assert_eq!(result.display_monthly_midpoint_yen, Some(290_100));
        assert_eq!(result.base_monthly_yen, Some(221_200));
        assert!(result.overtime_pay_included_in_display);
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
        assert!(!result.overtime_pay_included_in_display);
        // 「残業ほぼなし」は固定残業制度の記載ではないので None のまま
        assert_eq!(result.fixed_overtime, None, "notes={:?}", result.notes);
        assert!(!result.decomposed);
        assert_eq!(result.display_monthly_midpoint_yen, Some(290_100));
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
        // 手当込みの差を残業代と断定しない
        let notes = result.notes.join("\n");
        assert!(!notes.contains("差額"), "notes={notes}");
    }

    /// 「◯時間分の残業代」は数値の後ろの文脈から残業時間と判定する。
    /// 固定残業制度の語は無いので `fixed_overtime` は None のまま。
    #[test]
    fn overtime_hours_from_trailing_context() {
        let result = analyze("月給300,000円", "45時間分の残業代を含みます。");

        assert_eq!(result.overtime_hours_range, Some((45, 45)));
        assert!(result.overtime_pay_included_in_display);
        assert_eq!(result.fixed_overtime, None);
    }

    /// 時給表記は月額換算しない(単位換算は別モジュールの担当)。
    #[test]
    fn hourly_salary_is_not_converted() {
        let result = analyze("時給1,200円", "");

        assert_eq!(result.display_monthly_min_yen, None);
        assert_eq!(result.display_monthly_max_yen, None);
        assert_eq!(result.display_monthly_midpoint_yen, None);
        assert!(result.notes.iter().any(|n| n.contains("時給")));
    }

    /// 給与欄が空でもパニックせず、None を返す。
    #[test]
    fn empty_salary_text_is_safe() {
        let result = analyze("", "");

        assert_eq!(result.display_monthly_min_yen, None);
        assert_eq!(result.base_monthly_yen, None);
        assert_eq!(result.fixed_overtime, None);
        assert!(!result.overtime_pay_included_in_display);
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
