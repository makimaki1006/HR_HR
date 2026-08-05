//! 工程① 事実抽出。原文から不変項目(給与・勤務時間など8項目)を根拠引用つきで抜き、
//! 「引用が原文に一字一句実在するか」「値が引用から確認できるか」を機械照合する。
//!
//! Python 版 `scripts/job_creation_media_engine/`（`run_job_fact_poc.build_extract_prompt` と
//! `fact_validation.validate_extracted_facts` / `value_supported_by_quote`）の移植。
//! LLM には value と evidence_quote を返させるだけに制限し、判定は本モジュール(純粋関数)で行う。
//!
//! # Python 版との差異(意図的)
//! 契約(`jobgen_api_contract.md`)は結果を三値 status("verified"/"rejected"/"missing")で返し、
//! 「rejected でも value は保持しない(空にする)」と定める。Python 版は `FieldValidation` に
//! 元の value を保持したまま `ok=False` を立てるだけなので、次の3点で挙動が異なる:
//! 1. **rejected の value を空にする**（契約の明示要求）。Python は元 value を残す。
//! 2. **空 value は常に "missing"**。Python は「空 value + 原文に無い引用」を
//!    `evidence_not_found`(ok=False) にするが、主張されている値が無い以上ここでは欠落扱いにする。
//! 3. **オブジェクト以外/欠落は "missing"**。Python は非 dict を `invalid_shape`(ok=False) にするが、
//!    value を取り出せない以上欠落扱いにする(responseSchema で string 強制のため実運用では稀)。
//!
//! 正規化は Python `fact_validation.normalize_text` と同一手順を再現する(全角数字・全角カンマ/
//! ピリオド/コロンの ASCII 化 → 空白除去 → カンマ除去)。NFKC は使わない: Python が明示的な
//! 変換表しか使っておらず、NFKC を足すと全角括弧・記号の畳み込みで Python と挙動が乖離するため。

use std::collections::HashSet;

use serde_json::{json, Value};

use super::types::{ExtractedFacts, FactField, FACT_KEYS};

/// 各キーの日本語ラベル(Python `run_job_fact_poc.FIELD_LABELS` と一致)。
const FIELD_LABELS: [(&str, &str); 8] = [
    ("salary", "給与・賃金"),
    ("working_hours", "勤務時間・就業時間"),
    ("holidays", "休日"),
    ("work_location", "所在地・就業場所"),
    ("employment_type", "雇用形態"),
    ("insurance", "保険"),
    ("allowances", "手当"),
    ("required_qualifications", "必須資格・応募資格"),
];

/// 数値+単位ペアでの補完照合を許す項目(Python `fact_validation.NUMERIC_FACT_FIELDS`)。
const NUMERIC_FACT_FIELDS: [&str; 4] = ["salary", "working_hours", "holidays", "allowances"];

/// 原文長の上限(Python `source_text[:8000]` と同じ「文字数」でのカット)。
const SOURCE_LIMIT_CHARS: usize = 8000;

/// 抽出プロンプトを組み立てる(純粋)。Python `build_extract_prompt` の忠実移植。
///
/// 「固定スキーマを埋めるだけ」「一字一句そのまま引用」「要約・言い換え禁止」
/// 「evidence_quote 必須」「原文にない数字や条件を追加しない」の制約文を保持する。
/// なお「誰にでも当てはまる無難な表現は厳禁」は生成(コピー/原稿)工程向けの制約であり、
/// 事実の写しである本工程には意味を持たないため入れていない(Python 版に忠実)。
pub fn build_extract_prompt(source_text: &str) -> String {
    let labels = FIELD_LABELS
        .iter()
        .map(|(key, label)| format!("- {key}: {label}"))
        .collect::<Vec<_>>()
        .join("\n");
    let truncated: String = source_text.chars().take(SOURCE_LIMIT_CHARS).collect();

    format!(
        "次の求人票テキストから、不変項目だけを固定JSONで抽出してください。\n\
\n\
目的:\n\
- 給与、勤務時間、休日、所在地、雇用形態、保険、手当、必須資格などの労働条件を、生成AIが後続で書き換えないようにする。\n\
\n\
ルール:\n\
- 各項目は value と evidence_quote を返す。\n\
- evidence_quote は、元テキストから一字一句そのまま抜き出した短い引用にする。\n\
- evidence_quote は原文の連続した一節をそのまま抜く。離れた行をつなげて1つの引用にしない(箇条書きの明細を飛ばす場合は「・」区切りの各項目を原文と一字一句一致させること)。\n\
- 元テキストに根拠がない項目は value=\"\" evidence_quote=\"\" にする。\n\
- 要約・推測・言い換えは禁止。\n\
- 原文にない数字や条件を絶対に追加しない。\n\
- JSONのみ出力。\n\
\n\
項目:\n\
{labels}\n\
\n\
【求人票テキスト】\n\
{truncated}\n"
    )
}

/// 抽出結果の responseSchema(Python `EXTRACTION_SCHEMA` と同形)。
///
/// 8キーそれぞれが `{value, evidence_quote}`(いずれも string・必須)を持つオブジェクト。
pub fn response_schema() -> Value {
    let mut properties = serde_json::Map::new();
    for key in FACT_KEYS {
        properties.insert(
            key.to_string(),
            json!({
                "type": "object",
                "properties": {
                    "value": {"type": "string"},
                    "evidence_quote": {"type": "string"}
                },
                "required": ["value", "evidence_quote"]
            }),
        );
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": FACT_KEYS,
    })
}

/// LLM 応答(raw)を検証して [`ExtractedFacts`] に落とす。
///
/// 出力は必ず [`FACT_KEYS`] の8キー全てを含む。キー欠落・空 value → "missing"、
/// 引用が原文に実在しない/値が引用で確認できない → "rejected"(value は空にする)、
/// 引用実在+値整合 → "verified"。
pub fn verify(source_text: &str, raw: &Value) -> ExtractedFacts {
    let source_norm = normalize_text(source_text);
    let mut out = ExtractedFacts::new();

    for key in FACT_KEYS {
        let item = raw.get(key);
        // value / evidence_quote を string として取り出す(オブジェクト以外・欠落は空)。
        let value = item
            .and_then(|v| v.get("value"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let quote = item
            .and_then(|v| v.get("evidence_quote"))
            .and_then(Value::as_str)
            .unwrap_or("");

        let field = if value.is_empty() {
            // キー欠落・空 value はどちらも欠落扱い。
            FactField {
                value: String::new(),
                evidence_quote: String::new(),
                status: "missing".to_string(),
            }
        } else if quote.is_empty() {
            // 値はあるのに根拠引用が無い(Python: missing_evidence)。
            FactField {
                value: String::new(),
                evidence_quote: String::new(),
                status: "rejected".to_string(),
            }
        } else if !quote_exists_in_source(&source_norm, &normalize_text(quote)) {
            // 引用が原文に実在しない(Python: evidence_not_found)。引用は残す(レビュー用)。
            FactField {
                value: String::new(),
                evidence_quote: quote.to_string(),
                status: "rejected".to_string(),
            }
        } else if !value_supported_by_quote(key, value, quote) {
            // 値が引用から確認できない(Python: value_not_in_evidence)。
            FactField {
                value: String::new(),
                evidence_quote: quote.to_string(),
                status: "rejected".to_string(),
            }
        } else {
            FactField {
                value: value.to_string(),
                evidence_quote: quote.to_string(),
                status: "verified".to_string(),
            }
        };
        out.insert(key.to_string(), field);
    }
    out
}

/// 引用が原文に実在するかの判定。
///
/// 基本は正規化後の連続一致。それで一致しない場合に限り、箇条書きの「中抜き引用」を許容する
/// (2026-08-04 実データで発見: 原文「・通勤手当…／・資格手当（以下例）／◆明細…／・時間外手当」
///  に対し LLM が ◆ の明細行を飛ばして「・通勤手当… ・資格手当（以下例） ・時間外手当」と引用し、
///  実在する内容なのに rejected になっていた)。
///
/// 中抜き許容の条件は厳格に保つ: 引用を箇条書き記号で分割し、**全断片が原文に一字一句存在し、
/// かつ原文での出現順が引用と同じ**場合のみ一致とみなす。順序の入れ替え・存在しない断片は
/// 従来どおり rejected。断片の捏造・合成による誤検証は起きない。
fn quote_exists_in_source(source_norm: &str, quote_norm: &str) -> bool {
    if source_norm.contains(quote_norm) {
        return true;
    }
    // 箇条書き記号で分割 (正規化済みなので空白・改行は既に無い)
    let segments: Vec<&str> = quote_norm
        .split(['・', '◆', '●', '■', '▼'])
        .filter(|segment| segment.chars().count() >= 2)
        .collect();
    if segments.len() < 2 {
        return false;
    }
    let mut cursor = 0usize;
    for segment in segments {
        match source_norm[cursor..].find(segment) {
            Some(position) => cursor += position + segment.len(),
            None => return false,
        }
    }
    true
}

/// 照合用の正規化。Python `fact_validation.normalize_text` と同一手順。
///
/// 手順: 全角数字→ASCII / 全角カンマ・ピリオド・コロン→ASCII → 空白を全除去 → カンマを全除去。
/// (ピリオドとコロンは残す。カンマだけ除去するのは Python の `s.replace(",", "")` に合わせる。)
fn normalize_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let mapped = match ch {
            '０'..='９' => {
                // 全角数字を対応する ASCII 数字へ。
                char::from(b'0' + (ch as u32 - '０' as u32) as u8)
            }
            '，' => ',',
            '．' => '.',
            '：' => ':',
            other => other,
        };
        // Python: re.sub(r"\s+", "") — 全角空白(U+3000)含む空白を全除去。
        if mapped.is_whitespace() {
            continue;
        }
        // Python: s.replace(",", "") — 全角カンマも上で ',' 化済みなのでまとめて除去。
        if mapped == ',' {
            continue;
        }
        out.push(mapped);
    }
    out
}

/// 意味照合では改行を項目境界として残し、隣の行の数値を借用しない。
fn normalize_semantic_text(s: &str) -> String {
    s.split(['\r', '\n'])
        .map(normalize_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("。")
}

/// `\d+(?:\.\d+)?` 相当の数字トークンを(正規化後の文字列から)抽出する。
fn find_numbers(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            // 小数部 .\d+ は数字が続く場合のみ取り込む。
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(chars[start..i].iter().collect());
        } else {
            i += 1;
        }
    }
    out
}

/// 数字トークン集合。Python `fact_validation.numbers_in_text` と同一(万円/千円の倍数展開含む)。
///
/// 「万円」「千円」の判定は文字列全体に対して行う(Python の実装どおり粗い挙動を再現)。
fn numbers_in_text(text: &str) -> HashSet<String> {
    let s = normalize_text(text);
    let nums = find_numbers(&s);
    let mut expanded: HashSet<String> = nums.iter().cloned().collect();
    let has_manen = s.contains("万円");
    let has_senen = s.contains("千円");
    for n in &nums {
        if n.contains('.') {
            continue;
        }
        if let Ok(i) = n.parse::<i128>() {
            if has_manen {
                expanded.insert((i * 10_000).to_string());
            }
            if has_senen {
                expanded.insert((i * 1_000).to_string());
            }
        }
    }
    expanded
}

#[derive(Debug, Clone)]
struct NumberSpan {
    start: usize,
    end: usize,
    raw: String,
}

#[derive(Debug, Clone)]
struct MoneyEvidence {
    amount_yen: i128,
    pay_period: Option<&'static str>,
    salary_context: bool,
    allowance_context: bool,
}

const SALARY_CONTEXT: [&str; 12] = [
    "給与",
    "給料",
    "賃金",
    "月給",
    "月収",
    "年収",
    "年俸",
    "日給",
    "時給",
    "基本給",
    "固定給",
    "報酬",
];
const ALLOWANCE_CONTEXT: [&str; 11] = [
    "手当",
    "交通費",
    "通勤費",
    "補助",
    "支給",
    "祝い金",
    "一時金",
    "インセンティブ",
    "残業代",
    "賞与",
    "ボーナス",
];
const WORKING_HOURS_CONTEXT: [&str; 8] = [
    "勤務時間",
    "就業時間",
    "労働時間",
    "実働",
    "休憩時間",
    "勤務帯",
    "シフト",
    "勤務",
];
const HOLIDAY_CONTEXT: [&str; 8] = [
    "休日",
    "休暇",
    "休業",
    "週休",
    "有給",
    "年間休日",
    "休み",
    "公休",
];

fn find_number_spans(chars: &[char]) -> Vec<NumberSpan> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
        }
        spans.push(NumberSpan {
            start,
            end: i,
            raw: chars[start..i].iter().collect(),
        });
    }
    spans
}

fn starts_with_chars(chars: &[char], start: usize, needle: &str) -> bool {
    let needle = needle.chars().collect::<Vec<_>>();
    chars.get(start..start + needle.len()) == Some(needle.as_slice())
}

fn local_text(chars: &[char], start: usize, end: usize, radius: usize) -> String {
    fn is_boundary(ch: char) -> bool {
        matches!(ch, '。' | '、' | '；' | ';' | '\n' | '\r')
    }

    let mut clause_start = start;
    while clause_start > 0 && !is_boundary(chars[clause_start - 1]) {
        clause_start -= 1;
    }
    let mut clause_end = end;
    while clause_end < chars.len() && !is_boundary(chars[clause_end]) {
        clause_end += 1;
    }
    let window_start = start.saturating_sub(radius).max(clause_start);
    let window_end = (end + radius).min(clause_end);
    chars[window_start..window_end].iter().collect()
}

fn contains_any(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}

fn parse_scaled_number(raw: &str, multiplier: i128) -> Option<i128> {
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    let whole = whole.parse::<i128>().ok()?;
    if fraction.is_empty() {
        return whole.checked_mul(multiplier);
    }
    let scale = 10_i128.checked_pow(fraction.len() as u32)?;
    let fraction = fraction.parse::<i128>().ok()?;
    let numerator = whole.checked_mul(scale)?.checked_add(fraction)?;
    numerator.checked_mul(multiplier)?.checked_div(scale)
}

fn money_multiplier(chars: &[char], end: usize) -> Option<i128> {
    if starts_with_chars(chars, end, "万円") {
        Some(10_000)
    } else if starts_with_chars(chars, end, "千円") {
        Some(1_000)
    } else if starts_with_chars(chars, end, "円") {
        Some(1)
    } else if starts_with_chars(chars, end, "万") {
        Some(10_000)
    } else {
        None
    }
}

fn is_range_separator(chars: &[char]) -> bool {
    !chars.is_empty()
        && chars
            .iter()
            .all(|ch| matches!(ch, '〜' | '～' | '~' | '-' | '－' | '–' | '—'))
}

fn pay_period_near(local: &str) -> Option<&'static str> {
    let periods = [
        ("hourly", ["時給", "時間給"].as_slice()),
        ("daily", ["日給"].as_slice()),
        ("monthly", ["月給", "月収", "月額", "基本給"].as_slice()),
        ("annual", ["年収", "年俸", "年額"].as_slice()),
        ("bonus", ["賞与", "ボーナス"].as_slice()),
    ];
    let detected = periods
        .iter()
        .filter_map(|(period, terms)| contains_any(local, terms).then_some(*period))
        .collect::<Vec<_>>();
    (detected.len() == 1).then(|| detected[0])
}

fn money_evidence(text: &str) -> Vec<MoneyEvidence> {
    let normalized = normalize_semantic_text(text);
    let chars = normalized.chars().collect::<Vec<_>>();
    let spans = find_number_spans(&chars);
    let mut multipliers = spans
        .iter()
        .map(|span| money_multiplier(&chars, span.end))
        .collect::<Vec<_>>();

    // 「30〜40万円」の先頭値にも、同じ範囲の末尾に付いた単位だけを引き継ぐ。
    for index in 0..spans.len().saturating_sub(1) {
        if multipliers[index].is_none()
            && multipliers[index + 1].is_some()
            && is_range_separator(&chars[spans[index].end..spans[index + 1].start])
        {
            multipliers[index] = multipliers[index + 1];
        }
    }

    spans
        .iter()
        .zip(multipliers)
        .filter_map(|(span, multiplier)| {
            let amount_yen = parse_scaled_number(&span.raw, multiplier?)?;
            let local = local_text(&chars, span.start, span.end, 14);
            Some(MoneyEvidence {
                amount_yen,
                pay_period: pay_period_near(&local),
                salary_context: contains_any(&local, &SALARY_CONTEXT),
                allowance_context: contains_any(&local, &ALLOWANCE_CONTEXT),
            })
        })
        .collect()
}

fn salary_numbers_supported(value: &str, quote: &str) -> bool {
    let value_evidence = money_evidence(value);
    if value_evidence.is_empty() {
        return false;
    }
    let quote_evidence = money_evidence(quote)
        .into_iter()
        // 手当だけの金額を給与額として借用しない。給与文脈も併記される場合は残す。
        .filter(|item| !item.allowance_context || item.salary_context)
        .collect::<Vec<_>>();

    value_evidence.iter().all(|value_item| {
        quote_evidence.iter().any(|quote_item| {
            value_item.amount_yen == quote_item.amount_yen
                && match value_item.pay_period {
                    Some(period) => quote_item.pay_period == Some(period),
                    None => true,
                }
        })
    })
}

fn allowance_numbers_supported(value: &str, quote: &str) -> bool {
    let value_evidence = money_evidence(value);
    if value_evidence.is_empty() {
        return false;
    }
    let quote_norm = normalize_text(quote);
    let value_norm = normalize_text(value);
    let quote_evidence = money_evidence(quote)
        .into_iter()
        .filter(|item| item.allowance_context || quote_norm == value_norm)
        .collect::<Vec<_>>();
    value_evidence.iter().all(|value_item| {
        quote_evidence
            .iter()
            .any(|quote_item| value_item.amount_yen == quote_item.amount_yen)
    })
}

fn unit_numbers(
    text: &str,
    units: &[(&str, &str)],
    required_context: Option<&[&str]>,
) -> Vec<(String, String)> {
    let normalized = normalize_semantic_text(text);
    let chars = normalized.chars().collect::<Vec<_>>();
    find_number_spans(&chars)
        .into_iter()
        .filter_map(|span| {
            let item = units
                .iter()
                .find(|(suffix, _)| starts_with_chars(&chars, span.end, suffix))
                .map(|(_, kind)| (span.raw, (*kind).to_string()))?;
            let context_matches = required_context.is_none_or(|terms| {
                contains_any(&local_text(&chars, span.start, span.end, 14), terms)
            });
            context_matches.then_some(item)
        })
        .collect()
}

fn holiday_numbers_supported(value: &str, quote: &str) -> bool {
    let value_numbers = unit_numbers(value, &[("日", "days")], None);
    if value_numbers.is_empty() {
        return false;
    }
    let quote_norm = normalize_text(quote);
    let value_norm = normalize_text(value);
    let required_context = (quote_norm != value_norm).then_some(HOLIDAY_CONTEXT.as_slice());
    let quote_numbers = unit_numbers(quote, &[("日", "days")], required_context);
    value_numbers
        .iter()
        .all(|value_item| quote_numbers.contains(value_item))
}

fn clock_values(text: &str, required_context: Option<&[&str]>) -> Vec<String> {
    let normalized = normalize_semantic_text(text);
    let chars = normalized.chars().collect::<Vec<_>>();
    let spans = find_number_spans(&chars);
    let mut values = Vec::new();
    for pair in spans.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        let context_matches = required_context.is_none_or(|terms| {
            contains_any(&local_text(&chars, left.start, right.end, 14), terms)
        });
        if chars.get(left.end) == Some(&':') && right.start == left.end + 1 && context_matches {
            values.push(format!("{}:{}", left.raw, right.raw));
        }
    }
    values
}

fn working_hours_numbers_supported(value: &str, quote: &str) -> bool {
    let mut value_numbers = unit_numbers(
        value,
        &[
            ("時間", "duration_hours"),
            ("時", "clock_hours"),
            ("分", "minutes"),
        ],
        None,
    );
    value_numbers.extend(
        clock_values(value, None)
            .into_iter()
            .map(|clock| (clock, "clock".to_string())),
    );
    if value_numbers.is_empty() {
        return false;
    }

    let quote_norm = normalize_text(quote);
    let value_norm = normalize_text(value);
    let required_context = (quote_norm != value_norm).then_some(WORKING_HOURS_CONTEXT.as_slice());
    let mut quote_numbers = unit_numbers(
        quote,
        &[
            ("時間", "duration_hours"),
            ("時", "clock_hours"),
            ("分", "minutes"),
        ],
        required_context,
    );
    quote_numbers.extend(
        clock_values(quote, required_context)
            .into_iter()
            .map(|clock| (clock, "clock".to_string())),
    );
    value_numbers
        .iter()
        .all(|value_item| quote_numbers.contains(value_item))
}

fn numeric_semantics_supported(field_name: &str, value: &str, quote: &str) -> bool {
    match field_name {
        "salary" => salary_numbers_supported(value, quote),
        "working_hours" => working_hours_numbers_supported(value, quote),
        "holidays" => holiday_numbers_supported(value, quote),
        "allowances" => allowance_numbers_supported(value, quote),
        _ => false,
    }
}

fn non_numeric_field_context_supported(field_name: &str, quote: &str) -> bool {
    let quote = normalize_text(quote);
    match field_name {
        "salary" => contains_any(&quote, &SALARY_CONTEXT),
        "working_hours" => contains_any(&quote, &WORKING_HOURS_CONTEXT),
        "holidays" => contains_any(&quote, &HOLIDAY_CONTEXT),
        "allowances" => contains_any(&quote, &ALLOWANCE_CONTEXT),
        _ => true,
    }
}

/// 抽出値が根拠引用内で確認できるか。Python `value_supported_by_quote` の移植。
///
/// 数値系項目は、同じ数字だけでなく単位・項目文脈・給与周期まで一致する場合だけ許容する。
fn value_supported_by_quote(field_name: &str, value: &str, quote: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let quote_norm = normalize_text(quote);
    let value_norm = normalize_text(value);
    if value_norm.is_empty() {
        return true;
    }
    if NUMERIC_FACT_FIELDS.contains(&field_name) {
        let has_numbers = !numbers_in_text(value).is_empty();
        if has_numbers {
            return numeric_semantics_supported(field_name, value, quote);
        }
        return value_contained_or_enumerated(&quote_norm, &value_norm)
            && non_numeric_field_context_supported(field_name, quote);
    }
    value_contained_or_enumerated(&quote_norm, &value_norm)
}

/// 値が引用に含まれるかの判定。全体一致に加え、列挙型の値
/// (「通勤手当・資格手当・時間外手当」) は各要素が引用内に**同じ順序で**存在すれば
/// 支持されているとみなす (2026-08-04 実データ: 引用側の「（実費支給／規定あり）」等の
/// 注記を挟むと全体一致が成立しないため)。要素の捏造・順序入れ替えは通らない。
fn value_contained_or_enumerated(quote_norm: &str, value_norm: &str) -> bool {
    if quote_norm.contains(value_norm) {
        return true;
    }
    let parts: Vec<&str> = value_norm
        .split(['・', '、', ',', '/', '／'])
        .filter(|part| part.chars().count() >= 2)
        .collect();
    if parts.len() < 2 {
        return false;
    }
    let mut cursor = 0usize;
    for part in parts {
        match quote_norm[cursor..].find(part) {
            Some(position) => cursor += position + part.len(),
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(value: &str, quote: &str) -> Value {
        json!({"value": value, "evidence_quote": quote})
    }

    #[test]
    fn プロンプトに制約文と全キーが入る() {
        let p = build_extract_prompt("月給20万円");
        assert!(p.contains("一字一句そのまま抜き出した"), "{p}");
        assert!(p.contains("要約・推測・言い換えは禁止"), "{p}");
        assert!(p.contains("原文にない数字や条件を絶対に追加しない"), "{p}");
        assert!(p.contains("value=\"\" evidence_quote=\"\""), "{p}");
        // 8キーのラベルが列挙されている。
        assert!(p.contains("- salary: 給与・賃金"));
        assert!(p.contains("- required_qualifications: 必須資格・応募資格"));
        // 原文が埋め込まれている。
        assert!(p.contains("月給20万円"));
    }

    #[test]
    fn プロンプトは8000文字で原文を切る() {
        let long = "あ".repeat(9000);
        let p = build_extract_prompt(&long);
        let count = p.matches('あ').count();
        assert_eq!(count, 8000);
    }

    #[test]
    fn スキーマは8キーをvalueとevidence_quoteで持つ() {
        let s = response_schema();
        let props = s["properties"].as_object().unwrap();
        assert_eq!(props.len(), 8);
        for key in FACT_KEYS {
            let f = &props[key]["properties"];
            assert!(f.get("value").is_some());
            assert!(f.get("evidence_quote").is_some());
        }
        let required = s["required"].as_array().unwrap();
        assert_eq!(required.len(), 8);
    }

    // --- verify: 正常系 ---
    #[test]
    fn 正常系_引用実在かつ値整合はverified() {
        let source = "雇用形態 正社員 給与 月給192,000円〜195,000円";
        let raw = json!({
            "employment_type": item("正社員", "正社員"),
            "salary": item("月給192,000円", "月給192,000円〜195,000円"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["employment_type"].status, "verified");
        assert_eq!(facts["employment_type"].value, "正社員");
        assert_eq!(facts["salary"].status, "verified");
        assert_eq!(facts["salary"].value, "月給192,000円");
    }

    // --- verify: 捏造引用 ---
    #[test]
    fn 捏造引用は原文に無いのでrejected() {
        let source = "雇用形態 正社員";
        let raw = json!({
            "salary": item("月給30万円", "月給300,000円（原文に存在しない引用）"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["salary"].status, "rejected");
        // rejected は value を保持しない。
        assert_eq!(facts["salary"].value, "");
        // 引用はレビュー用に残す。
        assert_eq!(
            facts["salary"].evidence_quote,
            "月給300,000円（原文に存在しない引用）"
        );
    }

    // --- verify: 値が引用と無関係 ---
    #[test]
    fn 値が引用から確認できないとrejected() {
        // 引用は原文に実在するが、value の「必須」は引用内に無く、数値も無い(非数値項目)。
        let source = "応募資格 普通自動車免許 歓迎";
        let raw = json!({
            "required_qualifications": item("必須：大型自動車免許", "普通自動車免許 歓迎"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["required_qualifications"].status, "rejected");
        assert_eq!(facts["required_qualifications"].value, "");
    }

    #[test]
    fn 数値項目は引用の数字部分集合なら許容される() {
        // salary は NUMERIC_FACT_FIELDS。value の数字 {192000,195000} が引用の数字集合に含まれる。
        let source = "給与 192,000円〜195,000円 の範囲です";
        let raw = json!({
            // 部分文字列としては一致しないが、数字集合は引用に含まれる並べ替え表現。
            "salary": item("195,000円 192,000円", "192,000円〜195,000円"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["salary"].status, "verified");
    }

    #[test]
    fn numeric_facts_reject_numbers_borrowed_from_other_meanings() {
        let source = "給与は月給30万円。年間休日120日。勤務時間は実働8時間。通勤手当は5,000円。";
        let cases = [
            ("salary", "月給120万円"),
            ("salary", "月給5,000円"),
            ("holidays", "年間休日30日"),
            ("working_hours", "実働120時間"),
            ("allowances", "通勤手当120円"),
            ("allowances", "通勤手当30万円"),
        ];

        for (key, value) in cases {
            let raw = json!({ key: item(value, source) });
            let facts = verify(source, &raw);
            assert_eq!(facts[key].status, "rejected", "field={key}, value={value}");
        }
    }

    #[test]
    fn salary_rejects_same_amount_with_a_different_pay_period() {
        let source = "給与は年収300万円です。";
        let raw = json!({
            "salary": item("月給300万円", "給与は年収300万円です。"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["salary"].status, "rejected");
    }

    #[test]
    fn salary_accepts_equivalent_man_yen_and_yen_notation() {
        let source = "給与は月給300,000円です。";
        let raw = json!({
            "salary": item("月給30万円", "給与は月給300,000円です。"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["salary"].status, "verified");
        assert_eq!(facts["salary"].value, "月給30万円");
    }

    #[test]
    fn numeric_fact_rejects_an_exact_quote_with_the_wrong_field_semantics() {
        let source = "年間休日120日";
        let raw = json!({
            "salary": item("年間休日120日", "年間休日120日"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["salary"].status, "rejected");
    }

    #[test]
    fn salary_accepts_ranges_and_decimal_man_yen_notation() {
        let source = "給与は月給205,000円〜400,000円です。";
        let raw = json!({
            "salary": item("月給20.5万円〜40万円", "給与は月給205,000円〜400,000円です。"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["salary"].status, "verified");
    }

    #[test]
    fn working_hours_do_not_borrow_a_holiday_number_as_a_clock_time() {
        let source = "勤務時間は9:00〜18:00。年間休日120日。";
        let raw = json!({
            "working_hours": item("9:00〜12:00", source),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["working_hours"].status, "rejected");
    }

    #[test]
    fn holidays_do_not_borrow_days_from_a_trial_period() {
        for source in [
            "年間休日120日。試用期間30日。",
            "年間休日120日\n試用期間30日",
        ] {
            let raw = json!({
                "holidays": item("年間休日30日", source),
            });
            let facts = verify(source, &raw);
            assert_eq!(facts["holidays"].status, "rejected", "source={source}");
        }
    }

    #[test]
    fn working_hours_do_not_borrow_minutes_from_commuting_time() {
        let source = "勤務時間は実働8時間。通勤時間は60分。";
        let raw = json!({
            "working_hours": item("実働60分", source),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["working_hours"].status, "rejected");
    }

    // --- verify: キー欠落 ---
    #[test]
    fn キー欠落はmissing() {
        let source = "雇用形態 正社員";
        let raw = json!({ "employment_type": item("正社員", "正社員") });
        let facts = verify(source, &raw);
        // 8キー全て存在する。
        assert_eq!(facts.len(), 8);
        assert_eq!(facts["salary"].status, "missing");
        assert_eq!(facts["salary"].value, "");
        assert_eq!(facts["salary"].evidence_quote, "");
        // 空 value + 空 quote を明示的に渡した場合も missing。
        let raw2 = json!({ "holidays": item("", "") });
        let facts2 = verify(source, &raw2);
        assert_eq!(facts2["holidays"].status, "missing");
    }

    // --- verify: 空白・改行ゆらぎ ---
    #[test]
    fn 空白改行ゆらぎのある引用はverified() {
        // 原文はスペース区切り、引用は改行・全角空白混じり。正規化で空白が消えて一致する。
        let source = "就業時間 8時30分 〜 17時30分 休憩 60分";
        let raw = json!({
            "working_hours": item("8時30分〜17時30分", "8時30分\n〜　17時30分"),
        });
        let facts = verify(source, &raw);
        assert_eq!(facts["working_hours"].status, "verified");
        assert_eq!(facts["working_hours"].value, "8時30分〜17時30分");
    }

    // --- verify: 全キーverifiedの現実的な統合ケース(job_1 を縮約) ---
    #[test]
    fn 全キーverifiedの統合ケース() {
        let source = "雇用形態 正社員 \
賃金 192,000円〜195,000円 \
就業時間 8時30分〜17時30分 夜勤勤務あり \
休日 週休二日制 公休９日 \
就業場所 山形県山形市嶋南１－１０－１３ \
加入保険等 雇用保険，労災保険，健康保険，厚生年金 \
夜勤手当別途付与 ３，５００円／回 \
応募資格 普通自動車免許";
        let raw = json!({
            "salary": item("192,000円〜195,000円", "賃金 192,000円〜195,000円"),
            "working_hours": item("8時30分〜17時30分", "就業時間 8時30分〜17時30分 夜勤勤務あり"),
            "holidays": item("週休二日制 公休９日", "休日 週休二日制 公休９日"),
            "work_location": item("山形県山形市嶋南１－１０－１３", "就業場所 山形県山形市嶋南１－１０－１３"),
            "employment_type": item("正社員", "雇用形態 正社員"),
            "insurance": item("雇用保険，労災保険，健康保険，厚生年金", "加入保険等 雇用保険，労災保険，健康保険，厚生年金"),
            "allowances": item("３，５００円／回", "夜勤手当別途付与 ３，５００円／回"),
            "required_qualifications": item("普通自動車免許", "応募資格 普通自動車免許"),
        });
        let facts = verify(source, &raw);
        for key in FACT_KEYS {
            assert_eq!(
                facts[key].status, "verified",
                "key={key} が verified でない"
            );
            assert!(!facts[key].value.is_empty(), "key={key} の value が空");
        }
        // facts_to_text にも8項目全部が乗る。
        let text = super::super::types::facts_to_text(&facts);
        for key in FACT_KEYS {
            assert!(text.contains(&format!("{key}: ")), "text に {key} が無い");
        }
    }

    // --- 引用実在チェック(中抜き引用) ---
    // 2026-08-04 実データ回帰: 沖縄の実求人で、箇条書きの明細(◆行)を飛ばした引用が
    // rejected になり「手当 引用不一致」と表示された。原文はそのままの抜粋。
    #[test]
    fn 箇条書きの中抜き引用は順序が保たれていれば実在扱い() {
        let source = "※以下手当は別途支給いたします。\n【手当】\n・通勤手当（実費支給／規定あり）\n・資格手当（以下例）\n◆消防設備士（甲1-5類/乙6-7類）：各1,000円\n◆消防設備点検資格者（1級/2級）：各2,000円\n◆第2種電気工事士：1,500円\n・時間外手当\n【昇給・賞与】";
        let raw = serde_json::json!({
            "allowances": {
                "value": "通勤手当・資格手当・時間外手当",
                "evidence_quote": "・通勤手当（実費支給／規定あり） ・資格手当（以下例） ・時間外手当"
            }
        });
        let facts = verify(source, &raw);
        assert_eq!(
            facts["allowances"].status, "verified",
            "実在する箇条書きの中抜き引用が rejected になっている: {:?}",
            facts["allowances"]
        );

        // 逆証明1: 順序を入れ替えた引用は拒否される
        let raw = serde_json::json!({
            "allowances": {
                "value": "手当",
                "evidence_quote": "・時間外手当 ・通勤手当（実費支給／規定あり）"
            }
        });
        assert_eq!(
            verify(source, &raw)["allowances"].status,
            "rejected",
            "順序の入れ替わった引用を通してはいけない"
        );

        // 逆証明2: 原文に無い断片を混ぜた引用は拒否される
        let raw = serde_json::json!({
            "allowances": {
                "value": "手当",
                "evidence_quote": "・通勤手当（実費支給／規定あり） ・家族手当 ・時間外手当"
            }
        });
        assert_eq!(
            verify(source, &raw)["allowances"].status,
            "rejected",
            "存在しない断片(家族手当)を含む引用を通してはいけない"
        );

        // 逆証明3: 箇条書きでない普通の不一致引用は従来どおり拒否
        let raw = serde_json::json!({
            "allowances": {
                "value": "手当",
                "evidence_quote": "全ての手当を無制限に支給"
            }
        });
        assert_eq!(verify(source, &raw)["allowances"].status, "rejected");
    }

    // --- 正規化ユニット ---
    #[test]
    fn 正規化は全角数字とカンマと空白を畳む() {
        // 全角数字→ASCII、全角カンマ除去、空白除去、全角空白除去。
        // 全角スラッシュ／(U+FF0F)は Python の変換表に無いので畳まれない。
        assert_eq!(normalize_text("３，５００円　／回\n"), "3500円／回");
        // ピリオドは残る。
        assert_eq!(normalize_text("1．5 時間"), "1.5時間");
    }

    #[test]
    fn 万円千円の倍数展開() {
        // 「万円」を含むと整数に *10000 の変種が加わる。
        let nums = numbers_in_text("20万円");
        assert!(nums.contains("20"));
        assert!(nums.contains("200000"));
        // 千円: 千円→円変換の粗い展開。
        let nums2 = numbers_in_text("150千円");
        assert!(nums2.contains("150000"));
    }
}
